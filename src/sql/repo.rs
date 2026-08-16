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
