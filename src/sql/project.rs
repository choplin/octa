use crate::domain::project::{Project, ProjectState, ProjectStateType, ProjectTally};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn insert(
    pool: &SqlitePool,
    repository: i64,
    name: &str,
    summary: &str,
    description: &str,
    state: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"
        INSERT INTO projects
            (repository_id, id, name, summary, description, state)
        VALUES (?, (SELECT COALESCE(MAX(id), 0) + 1 FROM projects WHERE repository_id = ?),
                ?, ?, ?, ?)
        RETURNING id AS "id!: i64"
        "#,
        repository,
        repository,
        name,
        summary,
        description,
        state
    )
    .fetch_one(pool)
    .await?)
}

/// Every project read joins `project_states`, so the state's type arrives with
/// the project rather than being inferred from a flag stored beside the name.
/// The foreign key makes a project in an unconfigured state unrepresentable,
/// which is why the join is inner and no read guards against a missing row.
macro_rules! project_rows {
    ($pool:expr, $query:literal $(, $bind:expr)*) => {{
        let rows = sqlx::query!($query $(, $bind)*).fetch_all($pool).await?;
        rows.into_iter()
            .map(|row| {
                Ok(Project {
                    repository_id: row.repository_id,
                    repository: row.repository,
                    id: row.id,
                    name: row.name,
                    summary: row.summary,
                    description: row.description,
                    state: row.state,
                    state_type: ProjectStateType::parse(&row.state_type)?,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                })
            })
            .collect::<Result<Vec<Project>>>()
    }};
}

pub async fn get_by_id(pool: &SqlitePool, repository: i64, id: i64) -> Result<Option<Project>> {
    Ok(project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           WHERE p.repository_id = ? AND p.id = ?"#,
        repository,
        id
    )?
    .pop())
}

pub async fn get_by_name(
    pool: &SqlitePool,
    repository: i64,
    name: &str,
) -> Result<Option<Project>> {
    Ok(project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           WHERE p.repository_id = ? AND p.name = ? COLLATE NOCASE"#,
        repository,
        name
    )?
    .pop())
}

pub async fn list(
    pool: &SqlitePool,
    repository: Option<i64>,
    active_only: bool,
) -> Result<Vec<Project>> {
    // Keep each optional-filter shape as a static, SQLx-verified query. This is
    // intentionally repetitive: invalid column/type changes now fail prepare
    // or offline compilation rather than surfacing only at runtime.
    match (repository, active_only) {
        (Some(repository), true) => project_list_for_repository_active(pool, repository).await,
        (Some(repository), false) => project_list_for_repository_all(pool, repository).await,
        (None, true) => project_list_all_repositories_active(pool).await,
        (None, false) => project_list_all_repositories_all(pool).await,
    }
}

async fn project_list_for_repository_active(
    pool: &SqlitePool,
    repository: i64,
) -> Result<Vec<Project>> {
    project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           WHERE p.repository_id = ? AND s.type <> 'closed'
           ORDER BY r.name, p.id"#,
        repository
    )
}

async fn project_list_for_repository_all(
    pool: &SqlitePool,
    repository: i64,
) -> Result<Vec<Project>> {
    project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           WHERE p.repository_id = ?
           ORDER BY r.name, p.id"#,
        repository
    )
}

async fn project_list_all_repositories_active(pool: &SqlitePool) -> Result<Vec<Project>> {
    project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           WHERE s.type <> 'closed'
           ORDER BY r.name, p.id"#
    )
}

async fn project_list_all_repositories_all(pool: &SqlitePool) -> Result<Vec<Project>> {
    project_rows!(
        pool,
        r#"SELECT p.repository_id AS "repository_id!: i64", r.name AS "repository!: String",
                  p.id AS "id!: i64", p.name AS "name!: String",
                  p.summary AS "summary!: String", p.description AS "description!: String",
                  p.state AS "state!: String", s.type AS "state_type!: String",
                  p.created_at AS "created_at!: String",
                  p.updated_at AS "updated_at!: String"
           FROM projects p JOIN repositories r ON r.id = p.repository_id
                           JOIN project_states s ON s.name = p.state
           ORDER BY r.name, p.id"#
    )
}

pub async fn update(
    pool: &SqlitePool,
    repository: i64,
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
           WHERE repository_id = ? AND id = ?"#,
        name,
        summary,
        description,
        repository,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_state(pool: &SqlitePool, repository: i64, id: i64, state: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE projects SET state = ?, updated_at = datetime('now') WHERE repository_id = ? AND id = ?",
        state,
        repository,
        id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn issue_numbers(pool: &SqlitePool, repository: i64, id: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT issue_number AS "issue_number!: i64"
           FROM issue_projects
           WHERE repository_id = ? AND project_id = ?
           ORDER BY issue_number"#,
        repository,
        id
    )
    .fetch_all(pool)
    .await?)
}

pub async fn tally(pool: &SqlitePool, repository: i64, id: i64) -> Result<ProjectTally> {
    let rows = sqlx::query!(
        r#"SELECT s.type = 'closed' AS "is_closed!: bool",
                  COUNT(*) AS "count!: i64"
           FROM issue_projects ip
           JOIN issues i ON i.repository_id = ip.repository_id AND i.number = ip.issue_number
           JOIN issue_states s ON s.name = i.state
           WHERE ip.repository_id = ? AND ip.project_id = ?
           GROUP BY s.type = 'closed'"#,
        repository,
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

/// Seed the default project state set, but only when none is configured at all.
///
/// States are global, so this runs once for the store rather than once per
/// repository. A store whose states were already customized keeps exactly the
/// set it has. The shape mirrors `issue::seed_default_states`, defaults and
/// all: one state per type plus the second way an outcome ends, with the
/// defaults named outright rather than inferred from insertion order.
pub async fn seed_default_states(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let configured = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM project_states"#)
        .fetch_one(&mut *tx)
        .await?;
    if configured != 0 {
        return Ok(());
    }
    for (state, state_type) in [
        ("open", "open"),
        ("closed", "closed"),
        ("not planned", "closed"),
    ] {
        sqlx::query!(
            "INSERT INTO project_states (name, type) VALUES (?, ?)",
            state,
            state_type
        )
        .execute(&mut *tx)
        .await?;
    }
    for (state_type, state) in [("open", "open"), ("closed", "closed")] {
        sqlx::query!(
            "INSERT INTO project_state_defaults (type, name) VALUES (?, ?)",
            state_type,
            state
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// List every configured project state in lifecycle order.
///
/// States carry no stored ordinal, so the order is derived: by type, then the
/// type's default first, then by name.
pub async fn list_states(pool: &SqlitePool) -> Result<Vec<ProjectState>> {
    sqlx::query!(r#"SELECT s.name AS "name!: String", s.type AS "state_type!: String", (d.name IS NOT NULL) AS "is_default!: i64" FROM project_states s LEFT JOIN project_state_defaults d ON d.name = s.name ORDER BY CASE s.type WHEN 'open' THEN 0 ELSE 1 END, (d.name IS NOT NULL) DESC, s.name"#)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(ProjectState {
                name: row.name,
                state_type: ProjectStateType::parse(&row.state_type)?,
                is_default: row.is_default != 0,
            })
        })
        .collect()
}

/// Create a state, optionally taking its type's default.
///
/// Both writes share one transaction, so a state that is meant to be its type's
/// default never exists without being one. Nothing in the schema hands the
/// default out on its own; the caller decides, which is what keeps a restored
/// dump's defaults intact.
pub async fn insert_state(
    pool: &SqlitePool,
    name: &str,
    state_type: ProjectStateType,
    default: bool,
) -> Result<()> {
    let type_value = state_type.as_str();
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "INSERT INTO project_states (name, type) VALUES (?, ?)",
        name,
        type_value
    )
    .execute(&mut *tx)
    .await?;
    if default {
        set_default_state_tx(&mut tx, name, state_type).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The state `state_type` hands out when a verb is given no explicit target.
pub async fn default_state(
    pool: &SqlitePool,
    state_type: ProjectStateType,
) -> Result<Option<String>> {
    let type_value = state_type.as_str();
    Ok(sqlx::query_scalar!(
        "SELECT name FROM project_state_defaults WHERE type = ?",
        type_value
    )
    .fetch_optional(pool)
    .await?)
}

/// Every configured state of one type, in listing order.
pub async fn states_of_type(
    pool: &SqlitePool,
    state_type: ProjectStateType,
) -> Result<Vec<String>> {
    let type_value = state_type.as_str();
    Ok(sqlx::query_scalar!(
        "SELECT s.name FROM project_states s LEFT JOIN project_state_defaults d ON d.name = s.name WHERE s.type = ? ORDER BY (d.name IS NOT NULL) DESC, s.name",
        type_value
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_state(pool: &SqlitePool, name: &str) -> Result<Option<ProjectState>> {
    sqlx::query!(r#"SELECT s.name AS "name!: String", s.type AS "state_type!: String", (d.name IS NOT NULL) AS "is_default!: i64" FROM project_states s LEFT JOIN project_state_defaults d ON d.name = s.name WHERE s.name = ?"#, name)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            Ok(ProjectState {
                name: row.name,
                state_type: ProjectStateType::parse(&row.state_type)?,
                is_default: row.is_default != 0,
            })
        })
        .transpose()
}

pub async fn state_exists(pool: &SqlitePool, name: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM project_states WHERE name = ?"#,
        name
    )
    .fetch_one(pool)
    .await?
        > 0)
}

/// Count projects in a state across every repository.
///
/// States are global, so deleting or renaming one reaches every repository's
/// projects, not just the active one.
pub async fn count_projects_in_state(pool: &SqlitePool, name: &str) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM projects WHERE state = ?"#,
        name
    )
    .fetch_one(pool)
    .await?)
}

/// Rename a state.
///
/// `projects.state` references the name with `ON UPDATE CASCADE`, so every
/// project in the state follows in the same statement, across every repository.
pub async fn rename_state(pool: &SqlitePool, from: &str, to: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE project_states SET name = ? WHERE name = ?",
        to,
        from
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a state, first moving any projects that reference it to `move_to`.
///
/// The state is global, so this reaches every repository's projects. Both
/// statements share one transaction because `ON DELETE RESTRICT` rejects the
/// delete until the last project has left the state.
pub async fn delete_state(pool: &SqlitePool, name: &str, move_to: Option<&str>) -> Result<()> {
    let mut tx = pool.begin().await?;
    if let Some(move_to) = move_to {
        sqlx::query!(
            "UPDATE projects SET state = ?, updated_at = datetime('now') WHERE state = ?",
            move_to,
            name
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!("DELETE FROM project_states WHERE name = ?", name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Make `name` the default state of its own type.
///
/// One row per type means one write: the previous default is replaced in place
/// rather than cleared and re-set, so the type is never momentarily without one.
pub async fn set_default_state(
    pool: &SqlitePool,
    name: &str,
    state_type: ProjectStateType,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    set_default_state_tx(&mut tx, name, state_type).await?;
    tx.commit().await?;
    Ok(())
}

async fn set_default_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    state_type: ProjectStateType,
) -> Result<()> {
    let type_value = state_type.as_str();
    sqlx::query!(
        "INSERT INTO project_state_defaults (type, name) VALUES (?, ?)
         ON CONFLICT(type) DO UPDATE SET name = excluded.name",
        type_value,
        name
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
