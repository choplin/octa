use anyhow::Result;
use sqlx::SqlitePool;

use crate::domain::repository::Repository;

pub async fn list(pool: &SqlitePool) -> Result<Vec<Repository>> {
    crate::sql::repository::list(pool).await
}
