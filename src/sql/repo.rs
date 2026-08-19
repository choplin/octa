use anyhow::{bail, Result};
use sqlx::SqlitePool;

use crate::domain::repo::Repo;

pub async fn upsert(pool: &SqlitePool, path: &str, name: &str) -> Result<i64> {
    sqlx::query!(
        "INSERT INTO repos (path, name) VALUES (?, ?) ON CONFLICT(path) DO NOTHING",
        path,
        name
    )
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar!(r#"SELECT id AS "id!: i64" FROM repos WHERE path = ?"#, path)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

pub async fn by_name(pool: &SqlitePool, name: &str) -> Result<i64> {
    let rows = sqlx::query_scalar!(r#"SELECT id AS "id!: i64" FROM repos WHERE name = ?"#, name)
        .fetch_all(pool)
        .await?;
    match rows.len() {
        0 => bail!("no repository named {name:?} in the store"),
        1 => Ok(rows[0]),
        n => bail!("{n} repositories named {name:?}; identity is ambiguous"),
    }
}

/// Every repository octa has recorded, with its Issue counts by state type.
///
/// The counts are correlated subqueries rather than joins: a repository with no
/// Issues must still appear, and that is exactly the case a `--all-repos`
/// aggregate over Issues cannot produce.
pub async fn list(pool: &SqlitePool) -> Result<Vec<Repo>> {
    Ok(sqlx::query_as!(
        Repo,
        r#"
        SELECT
            r.name         AS "name!: String",
            r.path         AS "path!: String",
            r.created_at   AS "created_at!: String",
            (
                SELECT COUNT(*)
                FROM issues i
                JOIN issue_states s ON s.name = i.state
                WHERE i.repo_id = r.id AND s.type = 'open'
            ) AS "open_issues!: i64",
            (
                SELECT COUNT(*)
                FROM issues i
                JOIN issue_states s ON s.name = i.state
                WHERE i.repo_id = r.id AND s.type = 'in progress'
            ) AS "in_progress_issues!: i64"
        FROM
            repos r
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

    /// The whole reason this listing exists: a repository with no Issues is
    /// invisible to any aggregate over Issues, so it has to come from `repos`.
    #[tokio::test]
    async fn lists_a_repository_that_holds_no_issues() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repos (id, path, name) VALUES (1, '/a', 'alpha')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO repos (id, path, name) VALUES (2, '/b', 'beta')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(
            "INSERT INTO issues (repo_id, number, title, state) VALUES (1, 1, 'Issue', 'open')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let repos = super::list(&pool).await.unwrap();

        let names: Vec<_> = repos.iter().map(|repo| repo.name.as_str()).collect();
        assert_eq!(names, ["alpha", "beta"]);
        assert_eq!((repos[1].open_issues, repos[1].in_progress_issues), (0, 0));
    }

    /// Each count is its state's type, not a state name, so a second state of a
    /// type lands in the same column as the first, and both closed states are
    /// left out rather than only the one named `closed`.
    #[tokio::test]
    async fn counts_group_states_by_their_type() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repos (id, path, name) VALUES (1, '/a', 'alpha')")
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
                "INSERT INTO issues (repo_id, number, title, state) VALUES (1, ?, 'Issue', ?)",
            )
            .bind(number)
            .bind(state)
            .execute(&pool)
            .await
            .unwrap();
        }

        let repos = super::list(&pool).await.unwrap();

        assert_eq!((repos[0].open_issues, repos[0].in_progress_issues), (2, 1));
    }

    /// `name` carries no unique constraint, so the listing must show both rows
    /// rather than collapsing the ambiguity `by_name` later reports.
    #[tokio::test]
    async fn lists_both_repositories_that_share_a_name() {
        let pool = pool().await;
        sqlx::query("INSERT INTO repos (id, path, name) VALUES (1, '/one/a', 'a')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO repos (id, path, name) VALUES (2, '/two/a', 'a')")
            .execute(&pool)
            .await
            .unwrap();

        let repos = super::list(&pool).await.unwrap();

        let paths: Vec<_> = repos.iter().map(|repo| repo.path.as_str()).collect();
        assert_eq!(paths, ["/one/a", "/two/a"]);
    }
}
