use anyhow::Result;
use sqlx::SqlitePool;

use crate::domain::repo::Repo;

pub async fn list(pool: &SqlitePool) -> Result<Vec<Repo>> {
    crate::sql::repo::list(pool).await
}
