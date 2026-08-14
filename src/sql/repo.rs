use anyhow::{bail, Result};
use sqlx::SqlitePool;

pub async fn upsert(pool: &SqlitePool, identity_key: &str, name: &str) -> Result<i64> {
    sqlx::query!(
        "INSERT INTO repos (identity_key, name) VALUES (?, ?) ON CONFLICT(identity_key) DO NOTHING",
        identity_key,
        name
    )
    .execute(pool)
    .await?;
    let id = sqlx::query_scalar!(
        r#"SELECT id AS "id!: i64" FROM repos WHERE identity_key = ?"#,
        identity_key
    )
    .fetch_one(pool)
    .await?;
    // Seed only a repository that has no states at all. `upsert` runs on every
    // command, and a repository whose states were already customized must keep
    // exactly the set it has.
    let configured = sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM issue_states WHERE repo_id = ?"#,
        id
    )
    .fetch_one(pool)
    .await?;
    if configured == 0 {
        // Linear's default workflow, minus its `duplicate` status type which
        // `issue_states.status_type` does not model. New issues start in Backlog.
        for (state, status_type, starting, terminal) in [
            ("Backlog", "backlog", 1, 0),
            ("Todo", "unstarted", 0, 0),
            ("In Progress", "started", 0, 0),
            ("In Review", "started", 0, 0),
            ("Done", "completed", 0, 1),
            ("Canceled", "canceled", 0, 1),
        ] {
            sqlx::query!(
                "INSERT OR IGNORE INTO issue_states (repo_id, name, status_type, is_starting, is_terminal) VALUES (?, ?, ?, ?, ?)",
                id, state, status_type, starting, terminal
            )
            .execute(pool)
            .await?;
        }
    }
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
