use crate::domain::project::{Project, ProjectTally};
use anyhow::Result;
use sqlx::SqlitePool;

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    summary: &str,
    description: &str,
    state: &str,
    closed: bool,
) -> Result<i64> {
    let closed = closed as i64;
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO projects
            (repo_id, id, name, summary, description, state, is_closed)
        VALUES (?, (SELECT COALESCE(MAX(id), 0) + 1 FROM projects WHERE repo_id = ?),
                ?, ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        repo,
        repo,
        name,
        summary,
        description,
        state,
        closed
    )
    .fetch_one(pool)
    .await?)
}

pub async fn get_by_id(pool: &SqlitePool, repo: i64, id: i64) -> Result<Option<Project>> {
    Ok(sqlx::query_as!(
        Project,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE p.repo_id = ? AND p.id = ?"#,
        repo,
        id
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn get_by_name(pool: &SqlitePool, repo: i64, name: &str) -> Result<Option<Project>> {
    Ok(sqlx::query_as!(
        Project,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE p.repo_id = ? AND p.name = ? COLLATE NOCASE"#,
        repo,
        name
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, repo: Option<i64>, active_only: bool) -> Result<Vec<Project>> {
    // Keep each optional-filter shape as a static, SQLx-verified query. This is
    // intentionally repetitive: invalid column/type changes now fail prepare
    // or offline compilation rather than surfacing only at runtime.
    let projects = match (repo, active_only) {
        (Some(repo), true) => project_list_for_repo_active(pool, repo).await?,
        (Some(repo), false) => project_list_for_repo_all(pool, repo).await?,
        (None, true) => project_list_all_repos_active(pool).await?,
        (None, false) => project_list_all_repos_all(pool).await?,
    };
    Ok(projects)
}

macro_rules! project_list_query {
    ($pool:expr, $query:literal $(, $bind:expr)*) => {
        sqlx::query_as!(
            Project,
            $query
            $(, $bind)*
        )
        .fetch_all($pool)
        .await
    };
}

async fn project_list_for_repo_active(pool: &SqlitePool, repo: i64) -> Result<Vec<Project>> {
    Ok(project_list_query!(
        pool,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE p.repo_id = ? AND p.is_closed = 0
           ORDER BY r.name, p.id"#,
        repo
    )?)
}

async fn project_list_for_repo_all(pool: &SqlitePool, repo: i64) -> Result<Vec<Project>> {
    Ok(project_list_query!(
        pool,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE p.repo_id = ?
           ORDER BY r.name, p.id"#,
        repo
    )?)
}

async fn project_list_all_repos_active(pool: &SqlitePool) -> Result<Vec<Project>> {
    Ok(project_list_query!(
        pool,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE p.is_closed = 0
           ORDER BY r.name, p.id"#
    )?)
}

async fn project_list_all_repos_all(pool: &SqlitePool) -> Result<Vec<Project>> {
    Ok(project_list_query!(
        pool,
        r#"SELECT p.repo_id AS "repo_id!: i64", r.name AS "repo!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", p.is_closed AS "is_closed!: bool",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repos r ON r.id = p.repo_id
           WHERE 1 = 1
           ORDER BY r.name, p.id"#
    )?)
}

pub async fn update(
    pool: &SqlitePool,
    repo: i64,
    id: i64,
    name: Option<&str>,
    summary: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE projects SET
               name = COALESCE(?, name),
               summary = COALESCE(?, summary),
               description = COALESCE(?, description),
               updated_at = datetime('now')
           WHERE repo_id = ? AND id = ?"#,
        name,
        summary,
        description,
        repo,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(
    pool: &SqlitePool,
    repo: i64,
    id: i64,
    state: &str,
    closed: bool,
) -> Result<()> {
    let closed = closed as i64;
    sqlx::query!(
        "UPDATE projects SET state = ?, is_closed = ?, updated_at = datetime('now') WHERE repo_id = ? AND id = ?",
        state,
        closed,
        repo,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn issue_numbers(pool: &SqlitePool, repo: i64, id: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT issue_number AS "issue_number!: i64"
           FROM issue_projects
           WHERE repo_id = ? AND project_id = ?
           ORDER BY issue_number"#,
        repo,
        id
    )
    .fetch_all(pool)
    .await?)
}

pub async fn tally(pool: &SqlitePool, repo: i64, id: i64) -> Result<ProjectTally> {
    let rows = sqlx::query!(
        r#"SELECT COALESCE(s.is_closed, 0) AS "is_closed!: bool",
                  COUNT(*) AS "count!: i64"
           FROM issue_projects ip
           JOIN issues i ON i.repo_id = ip.repo_id AND i.number = ip.issue_number
           LEFT JOIN issue_states s ON s.name = i.state
           WHERE ip.repo_id = ? AND ip.project_id = ?
           GROUP BY COALESCE(s.is_closed, 0)"#,
        repo,
        id
    )
    .fetch_all(pool)
    .await?;
    let mut tally = ProjectTally::default();
    for row in rows {
        if row.is_closed {
            tally.closed = row.count;
        } else {
            tally.open = row.count;
        }
        tally.total += row.count;
    }
    Ok(tally)
}
