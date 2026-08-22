use crate::domain::milestone::ProjectMilestone;
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

struct MilestoneRow {
    repository_id: i64,
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
            repository_id: row.repository_id,
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

#[allow(clippy::too_many_arguments)]
pub async fn insert(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
    name: &str,
    description: &str,
    status: &str,
    position: Option<i64>,
    start_date: Option<&str>,
    target_date: Option<&str>,
) -> Result<i64> {
    let id = sqlx::query_scalar!(
        r#"INSERT INTO project_milestones
               (repository_id, project_id, id, position, name, description, status,
                start_date, target_date)
           VALUES (
               ?, ?,
               (SELECT COALESCE(MAX(id), 0) + 1 FROM project_milestones
                WHERE repository_id = ? AND project_id = ?),
               COALESCE(?, (SELECT COALESCE(MAX(position), -1) + 1
                            FROM project_milestones
                            WHERE repository_id = ? AND project_id = ?)),
               ?, ?, ?, ?, ?
           )
           RETURNING id AS "id!: i64""#,
        repository,
        project,
        repository,
        project,
        position,
        repository,
        project,
        name,
        description,
        status,
        start_date,
        target_date,
    )
    .fetch_one(pool)
    .await?;
    Ok(id)
}

pub async fn get_by_id(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
    id: i64,
) -> Result<Option<ProjectMilestone>> {
    Ok(sqlx::query_as!(
        MilestoneRow,
        r#"SELECT repository_id AS "repository_id!: i64", project_id AS "project_id!: i64",
                  id AS "id!: i64", position AS "position!: i64", name AS "name!: String",
                  description AS "description!: String", status AS "status!: String",
                  start_date AS "start_date?: String", target_date AS "target_date?: String",
                  created_at AS "created_at!: String", updated_at AS "updated_at!: String"
           FROM project_milestones
           WHERE repository_id = ? AND project_id = ? AND id = ?"#,
        repository,
        project,
        id
    )
    .fetch_optional(pool)
    .await?
    .map(Into::into))
}

pub async fn get_by_name(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
    name: &str,
) -> Result<Option<ProjectMilestone>> {
    Ok(sqlx::query_as!(
        MilestoneRow,
        r#"SELECT repository_id AS "repository_id!: i64", project_id AS "project_id!: i64",
                  id AS "id!: i64", position AS "position!: i64", name AS "name!: String",
                  description AS "description!: String", status AS "status!: String",
                  start_date AS "start_date?: String", target_date AS "target_date?: String",
                  created_at AS "created_at!: String", updated_at AS "updated_at!: String"
           FROM project_milestones
           WHERE repository_id = ? AND project_id = ? AND name = ? COLLATE NOCASE"#,
        repository,
        project,
        name
    )
    .fetch_optional(pool)
    .await?
    .map(Into::into))
}

pub async fn list(
    pool: &SqlitePool,
    repository: i64,
    project: i64,
) -> Result<Vec<ProjectMilestone>> {
    Ok(sqlx::query_as!(
        MilestoneRow,
        r#"SELECT repository_id AS "repository_id!: i64", project_id AS "project_id!: i64",
                  id AS "id!: i64", position AS "position!: i64", name AS "name!: String",
                  description AS "description!: String", status AS "status!: String",
                  start_date AS "start_date?: String", target_date AS "target_date?: String",
                  created_at AS "created_at!: String", updated_at AS "updated_at!: String"
           FROM project_milestones
           WHERE repository_id = ? AND project_id = ? ORDER BY position, id"#,
        repository,
        project
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(Into::into)
    .collect())
}

#[allow(clippy::too_many_arguments)]
pub async fn update(
    pool: &SqlitePool,
    repository: i64,
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
    sqlx::query!(
        r#"UPDATE project_milestones SET
               name = COALESCE(?, name),
               description = COALESCE(?, description),
               status = COALESCE(?, status),
               position = COALESCE(?, position),
               start_date = CASE WHEN ? THEN NULL ELSE COALESCE(?, start_date) END,
               target_date = CASE WHEN ? THEN NULL ELSE COALESCE(?, target_date) END,
               updated_at = datetime('now')
           WHERE repository_id = ? AND project_id = ? AND id = ?"#,
        name,
        description,
        status,
        position,
        clear_start_date,
        start_date,
        clear_target_date,
        target_date,
        repository,
        project,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_issue(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    project: i64,
    milestone: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_milestones
               (repository_id, issue_number, project_id, milestone_id)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(repository_id, issue_number) DO UPDATE SET
               project_id = excluded.project_id,
               milestone_id = excluded.milestone_id"#,
        repository,
        number,
        project,
        milestone
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_issue_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    project: i64,
    milestone: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_milestones
               (repository_id, issue_number, project_id, milestone_id)
           VALUES (?, ?, ?, ?)
           ON CONFLICT(repository_id, issue_number) DO UPDATE SET
               project_id = excluded.project_id,
               milestone_id = excluded.milestone_id"#,
        repository,
        number,
        project,
        milestone
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn clear_issue_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_milestones WHERE repository_id = ? AND issue_number = ?",
        repository,
        number
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
