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
    for (state, status_type, starting, terminal, pos, group, rank) in [
        ("Backlog", "backlog", 1, 0, 0, "backlog", 0),
        ("Todo", "unstarted", 0, 0, 1, "active", 10),
        ("In Progress", "started", 0, 0, 2, "active", 20),
        ("In Review", "started", 0, 0, 3, "active", 30),
        ("Done", "completed", 0, 1, 4, "terminal", 40),
        ("Canceled", "canceled", 0, 1, 5, "terminal", 50),
    ] {
        sqlx::query!(
            "INSERT OR IGNORE INTO issue_states (repo_id, name, status_type, is_starting, is_terminal, position) VALUES (?, ?, ?, ?, ?, ?)",
            id, state, status_type, starting, terminal, pos
        )
        .execute(pool)
        .await?;
        sqlx::query(
            "UPDATE issue_states SET workflow_group = ?, workflow_rank = ? WHERE repo_id = ? AND name = ?",
        )
        .bind(group)
        .bind(rank)
        .bind(id)
        .bind(state)
        .execute(pool)
        .await?;
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
