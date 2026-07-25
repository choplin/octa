use crate::domain::milestone::ProjectMilestone;
use anyhow::Result;
use sqlx::{FromRow, SqlitePool};

#[derive(FromRow)]
struct MilestoneRow {
    repo_id: i64,
    project_id: i64,
    id: i64,
    position: i64,
    name: String,
    description: String,
    status: String,
    start_date: Option<String>,
    target_date: Option<String>,
    created_at: String,
    updated_at: String,
}

impl From<MilestoneRow> for ProjectMilestone {
    fn from(row: MilestoneRow) -> Self {
        Self {
            repo_id: row.repo_id,
            project_id: row.project_id,
            id: row.id,
            position: row.position,
            name: row.name,
            description: row.description,
            status: row.status,
            start_date: row.start_date,
            target_date: row.target_date,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

const COLUMNS: &str = r#"repo_id, project_id, id, position, name, description,
                          status, start_date, target_date, created_at, updated_at"#;

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    repo: i64,
    project: i64,
    name: &str,
    description: &str,
    status: &str,
    position: Option<i64>,
    start_date: Option<&str>,
    target_date: Option<&str>,
) -> Result<i64> {
    let id = sqlx::query_scalar::<_, i64>(
        r#"INSERT INTO project_milestones
               (repo_id, project_id, id, position, name, description, status,
                start_date, target_date)
           VALUES (
               ?, ?,
               (SELECT COALESCE(MAX(id), 0) + 1 FROM project_milestones
                WHERE repo_id = ? AND project_id = ?),
               COALESCE(?, (SELECT COALESCE(MAX(position), -1) + 1
                            FROM project_milestones
                            WHERE repo_id = ? AND project_id = ?)),
               ?, ?, ?, ?, ?
           )
           RETURNING id"#,
    )
    .bind(repo)
    .bind(project)
    .bind(repo)
    .bind(project)
    .bind(position)
    .bind(repo)
    .bind(project)
    .bind(name)
    .bind(description)
    .bind(status)
    .bind(start_date)
    .bind(target_date)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn get_by_id(
    pool: &SqlitePool,
    repo: i64,
    project: i64,
    id: i64,
) -> Result<Option<ProjectMilestone>> {
    let query = format!(
        "SELECT {COLUMNS} FROM project_milestones \
         WHERE repo_id = ? AND project_id = ? AND id = ?"
    );
    Ok(sqlx::query_as::<_, MilestoneRow>(&query)
        .bind(repo)
        .bind(project)
        .bind(id)
        .fetch_optional(pool)
        .await?
        .map(Into::into))
}

pub async fn get_by_name(
    pool: &SqlitePool,
    repo: i64,
    project: i64,
    name: &str,
) -> Result<Option<ProjectMilestone>> {
    let query = format!(
        "SELECT {COLUMNS} FROM project_milestones \
         WHERE repo_id = ? AND project_id = ? AND name = ? COLLATE NOCASE"
    );
    Ok(sqlx::query_as::<_, MilestoneRow>(&query)
        .bind(repo)
        .bind(project)
        .bind(name)
        .fetch_optional(pool)
        .await?
        .map(Into::into))
}

pub async fn list(pool: &SqlitePool, repo: i64, project: i64) -> Result<Vec<ProjectMilestone>> {
    let query = format!(
        "SELECT {COLUMNS} FROM project_milestones \
         WHERE repo_id = ? AND project_id = ? ORDER BY position, id"
    );
    Ok(sqlx::query_as::<_, MilestoneRow>(&query)
        .bind(repo)
        .bind(project)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &SqlitePool,
    repo: i64,
    project: i64,
    id: i64,
    name: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
    position: Option<i64>,
    start_date: Option<&str>,
    target_date: Option<&str>,
    clear_start_date: bool,
    clear_target_date: bool,
) -> Result<()> {
    sqlx::query(
        r#"UPDATE project_milestones SET
               name = COALESCE(?, name),
               description = COALESCE(?, description),
               status = COALESCE(?, status),
               position = COALESCE(?, position),
               start_date = CASE WHEN ? THEN NULL ELSE COALESCE(?, start_date) END,
               target_date = CASE WHEN ? THEN NULL ELSE COALESCE(?, target_date) END,
               updated_at = datetime('now')
           WHERE repo_id = ? AND project_id = ? AND id = ?"#,
    )
    .bind(name)
    .bind(description)
    .bind(status)
    .bind(position)
    .bind(clear_start_date)
    .bind(start_date)
    .bind(clear_target_date)
    .bind(target_date)
    .bind(repo)
    .bind(project)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_issue(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    project: i64,
    milestone: i64,
) -> Result<()> {
    sqlx::query(
        r#"INSERT INTO issue_milestones
               (repo_id, issue_number, project_id, milestone_id)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(repo_id, issue_number) DO UPDATE SET
               project_id = excluded.project_id,
               milestone_id = excluded.milestone_id"#,
    )
    .bind(repo)
    .bind(number)
    .bind(project)
    .bind(milestone)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_issue(pool: &SqlitePool, repo: i64, number: i64) -> Result<()> {
    sqlx::query("DELETE FROM issue_milestones WHERE repo_id = ? AND issue_number = ?")
        .bind(repo)
        .bind(number)
        .execute(pool)
        .await?;
    Ok(())
}
