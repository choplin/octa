use crate::domain::label::{Label, LabelGroup};
use anyhow::{bail, Result};
use sqlx::SqlitePool;
pub async fn create_group(pool: &SqlitePool, repo: i64, name: &str, selection: &str) -> Result<()> {
    if selection != "single" && selection != "multi" {
        bail!("selection must be 'single' or 'multi'");
    }
    crate::sql::label::insert_group(pool, repo, name, selection).await
}

pub async fn create(pool: &SqlitePool, repo: i64, name: &str, group: Option<&str>) -> Result<()> {
    if let Some(group) = group {
        if !crate::sql::label::group_exists(pool, repo, group).await? {
            bail!("unknown label group {group:?}; create it first with `octa label group`");
        }
    }
    crate::sql::label::insert(pool, repo, name, group).await
}

pub async fn list(pool: &SqlitePool, repo: i64) -> Result<Vec<Label>> {
    crate::sql::label::list(pool, repo).await
}

pub async fn list_groups(pool: &SqlitePool, repo: i64) -> Result<Vec<LabelGroup>> {
    crate::sql::label::list_groups(pool, repo).await
}

pub async fn attach(pool: &SqlitePool, repo: i64, number: i64, label: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    if !crate::sql::label::issue_exists_tx(&mut tx, repo, number).await? {
        bail!("issue #{number} not found");
    }
    let group = crate::sql::label::label_group_tx(&mut tx, repo, label)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("unknown label {label:?}; create it first with `octa label create`")
        })?;
    if let Some(group) = group {
        if crate::sql::label::group_selection_tx(&mut tx, repo, &group).await? == "single" {
            crate::sql::label::replace_single_group(&mut tx, repo, number, &group).await?;
        }
    }
    crate::sql::label::attach(&mut tx, repo, number, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn detach(pool: &SqlitePool, repo: i64, number: i64, label: &str) -> Result<()> {
    crate::sql::label::detach(pool, repo, number, label).await
}
