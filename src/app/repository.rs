use anyhow::{bail, Result};
use sqlx::SqlitePool;

use crate::domain::repository::Repository;
use crate::sql::repository::RepositoryIdentity;

pub async fn list(pool: &SqlitePool) -> Result<Vec<Repository>> {
    crate::sql::repository::list(pool).await
}

pub async fn register(pool: &SqlitePool, path: &str, name: &str) -> Result<i64> {
    validate_registration(pool, path, name).await?;
    crate::sql::repository::insert(pool, path, name).await
}

pub async fn validate_registration(pool: &SqlitePool, path: &str, name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("repository name must not be empty");
    }
    if let Some(existing) = crate::sql::repository::by_path(pool, path).await? {
        bail!(
            "repository path {path:?} is already registered as {:?}; use `octa repository set` to change its name or `octa repository relocate` after moving it",
            existing.name,
        );
    }
    if let Some(existing) = crate::sql::repository::by_name_identity(pool, name).await? {
        bail!(
            "repository name {name:?} is already used by path {:?}",
            existing.path
        );
    }
    Ok(())
}

pub async fn identity_by_path(pool: &SqlitePool, path: &str) -> Result<Option<RepositoryIdentity>> {
    crate::sql::repository::by_path(pool, path).await
}

pub async fn identity_by_name(pool: &SqlitePool, name: &str) -> Result<RepositoryIdentity> {
    crate::sql::repository::by_name_identity(pool, name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no repository named {name:?} in the store"))
}

pub async fn set_name(
    pool: &SqlitePool,
    id: i64,
    old_name: &str,
    old_path: &str,
    new_name: &str,
) -> Result<()> {
    validate_name(pool, id, new_name).await?;
    crate::sql::repository::set_name(pool, id, old_name, old_path, new_name).await
}

pub async fn validate_name(pool: &SqlitePool, id: i64, new_name: &str) -> Result<()> {
    if new_name.trim().is_empty() {
        bail!("repository name must not be empty");
    }
    if let Some(existing) = crate::sql::repository::by_name_identity(pool, new_name).await? {
        if existing.id != id {
            bail!(
                "repository name {new_name:?} is already used by path {:?}",
                existing.path
            );
        }
    }
    Ok(())
}

pub async fn relocate(
    pool: &SqlitePool,
    name: &str,
    path: &str,
    configured_name: &str,
) -> Result<i64> {
    let repository = crate::sql::repository::by_name_identity(pool, name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no repository named {name:?} in the store"))?;
    if repository.name != configured_name {
        bail!(
            "repository name mismatch for {name:?}; the selected Git repository is configured as {configured_name:?}"
        );
    }
    if let Some(existing) = crate::sql::repository::by_path(pool, path).await? {
        if existing.id != repository.id {
            bail!(
                "repository path {path:?} is already registered as {:?}",
                existing.name
            );
        }
    }
    crate::sql::repository::relocate(
        pool,
        repository.id,
        &repository.name,
        &repository.path,
        path,
    )
    .await?;
    Ok(repository.id)
}
