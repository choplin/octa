use anyhow::{bail, Result};
use sqlx::SqlitePool;

use crate::domain::repository::Repository;

#[derive(Debug, Clone)]
pub struct RepositoryIdentity {
    pub id: i64,
    pub path: String,
    pub name: String,
}

pub async fn insert(pool: &SqlitePool, path: &str, name: &str) -> Result<i64> {
    let result = sqlx::query!(
        "INSERT INTO repositories (path, name) VALUES (?, ?)",
        path,
        name
    )
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

pub async fn by_name(pool: &SqlitePool, name: &str) -> Result<i64> {
    by_name_identity(pool, name)
        .await?
        .map(|repository| repository.id)
        .ok_or_else(|| anyhow::anyhow!("no repository named {name:?} in the store"))
}

pub async fn by_name_identity(pool: &SqlitePool, name: &str) -> Result<Option<RepositoryIdentity>> {
    Ok(sqlx::query_as!(
        RepositoryIdentity,
        r#"SELECT id AS "id!: i64", path AS "path!: String", name AS "name!: String"
           FROM repositories WHERE name = ?"#,
        name
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn by_path(pool: &SqlitePool, path: &str) -> Result<Option<RepositoryIdentity>> {
    Ok(sqlx::query_as!(
        RepositoryIdentity,
        r#"SELECT id AS "id!: i64", path AS "path!: String", name AS "name!: String"
           FROM repositories WHERE path = ?"#,
        path
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn set_name(
    pool: &SqlitePool,
    id: i64,
    old_name: &str,
    old_path: &str,
    name: &str,
) -> Result<()> {
    let result = sqlx::query!(
        "UPDATE repositories SET name = ?, updated_at = datetime('now') WHERE id = ? AND name = ? AND path = ?",
        name,
        id,
        old_name,
        old_path
    )
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        bail!("repository name changed concurrently");
    }
    Ok(())
}

pub async fn relocate(
    pool: &SqlitePool,
    id: i64,
    old_name: &str,
    old_path: &str,
    path: &str,
) -> Result<()> {
    let result = sqlx::query!(
        "UPDATE repositories SET path = ?, updated_at = datetime('now') WHERE id = ? AND name = ? AND path = ?",
        path,
        id,
        old_name,
        old_path
    )
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        bail!("repository changed concurrently while relocating it");
    }
    Ok(())
}

/// Every repository octa has recorded, with its Issue counts by state type.
///
/// The counts are correlated subqueries rather than joins: a repository with no
/// Issues must still appear, and that is exactly the case a `--all-repositories`
/// aggregate over Issues cannot produce.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Repository>> {
    Ok(sqlx::query_as!(
        Repository,
        r#"
        SELECT
            r.name         AS "name!: String",
            r.path         AS "path!: String",
            r.created_at   AS "created_at!: String",
            r.updated_at   AS "updated_at!: String",
            (
                SELECT COUNT(*)
                FROM issues i
                JOIN issue_states s ON s.name = i.state
                WHERE i.repository_id = r.id AND s.type = 'open'
            ) AS "open_issues!: i64",
            (
                SELECT COUNT(*)
                FROM issues i
                JOIN issue_states s ON s.name = i.state
                WHERE i.repository_id = r.id AND s.type = 'in progress'
            ) AS "in_progress_issues!: i64"
        FROM
            repositories r
        ORDER BY
            r.name, r.id
        "#
    )
    .fetch_all(pool)
    .await?)
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_init.sql"))
            .execute(&pool)
            .await
            .unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        pool
    }

    async fn set_repository_updated_at(pool: &SqlitePool, value: &str) {
        sqlx::query("UPDATE repositories SET updated_at = ? WHERE id = 1")
            .bind(value)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn repository_updated_at(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT updated_at FROM repositories WHERE id = 1")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    /// The whole reason this listing exists: a repository with no Issues is
    /// invisible to any aggregate over Issues, so it has to come from `repositories`.
    #[tokio::test]
    async fn lists_a_repository_that_holds_no_issues() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, '/a', 'alpha')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (2, '/b', 'beta')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO issues (repository_id, number, title, state) VALUES (1, 1, 'Issue', 'open')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repositories = super::list(&pool).await.unwrap();

        let names: Vec<_> = repositories
            .iter()
            .map(|repository| repository.name.as_str())
            .collect();
        assert_eq!(names, ["alpha", "beta"]);
        assert_eq!(
            (
                repositories[1].open_issues,
                repositories[1].in_progress_issues
            ),
            (0, 0)
        );
    }

    /// Each count is its state's type, not a state name, so a second state of a
    /// type lands in the same column as the first, and both closed states are
    /// left out rather than only the one named `closed`.
    #[tokio::test]
    async fn counts_group_states_by_their_type() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, '/a', 'alpha')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO issue_states (name, type) VALUES ('triage', 'open')")
            .execute(&pool)
            .await
            .unwrap();
        for (number, state) in [
            (1_i64, "open"),
            (2, "triage"),
            (3, "in progress"),
            (4, "closed"),
            (5, "not planned"),
        ] {
            sqlx::query(
                "INSERT INTO issues (repository_id, number, title, state) VALUES (1, ?, 'Issue', ?)",
            )
            .bind(number)
            .bind(state)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repositories = super::list(&pool).await.unwrap();

        assert_eq!(
            (
                repositories[0].open_issues,
                repositories[0].in_progress_issues
            ),
            (2, 1)
        );
    }

    /// Repository names are user-facing locators, so exact duplicates must
    /// conflict instead of making selection ambiguous.
    #[tokio::test]
    async fn rejects_duplicate_names() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, '/one/a', 'alpha')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (2, '/two/a', 'ALPHA')")
            .execute(&pool)
            .await
            .unwrap();
        let duplicate = sqlx::query(
            "INSERT INTO repositories (id, path, name) VALUES (3, '/three/a', 'alpha')",
        )
        .execute(&pool)
        .await;
        assert!(duplicate.is_err());
    }

    /// Name and path form one metadata state: a command using a stale snapshot
    /// must not overwrite the other command's successful change.
    #[tokio::test]
    async fn name_and_path_updates_reject_each_others_stale_snapshots() {
        let relocated_first = pool().await;
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, '/old', 'alpha')")
            .execute(&relocated_first)
            .await
            .unwrap();
        set_repository_updated_at(&relocated_first, "2000-01-01 00:00:00").await;
        super::relocate(&relocated_first, 1, "alpha", "/old", "/new")
            .await
            .unwrap();
        assert_ne!(
            repository_updated_at(&relocated_first).await,
            "2000-01-01 00:00:00"
        );
        set_repository_updated_at(&relocated_first, "2001-01-01 00:00:00").await;
        assert!(
            super::set_name(&relocated_first, 1, "alpha", "/old", "beta")
                .await
                .is_err()
        );
        let after_relocate = super::by_name_identity(&relocated_first, "alpha")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after_relocate.path, "/new");
        assert_eq!(
            repository_updated_at(&relocated_first).await,
            "2001-01-01 00:00:00"
        );

        let renamed_first = pool().await;
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, '/old', 'alpha')")
            .execute(&renamed_first)
            .await
            .unwrap();
        set_repository_updated_at(&renamed_first, "2000-01-01 00:00:00").await;
        super::set_name(&renamed_first, 1, "alpha", "/old", "beta")
            .await
            .unwrap();
        assert_ne!(
            repository_updated_at(&renamed_first).await,
            "2000-01-01 00:00:00"
        );
        set_repository_updated_at(&renamed_first, "2001-01-01 00:00:00").await;
        assert!(super::relocate(&renamed_first, 1, "alpha", "/old", "/new")
            .await
            .is_err());
        let after_set = super::by_name_identity(&renamed_first, "beta")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after_set.path, "/old");
        assert_eq!(
            repository_updated_at(&renamed_first).await,
            "2001-01-01 00:00:00"
        );
    }
}
