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
            bail!("unknown label group {group:?}; create it first with `octa config label-group create --target issue`");
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

pub async fn attach(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    label: &str,
    lease: Option<&str>,
) -> Result<()> {
    if crate::sql::issue::get(pool, repo, number).await?.is_none() {
        bail!("issue #{number} not found");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    let group = crate::sql::label::label_group_tx(&mut tx, repo, label)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("unknown label {label:?}; create it first with `octa config label create --target issue`")
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

pub async fn detach(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    label: &str,
    lease: Option<&str>,
) -> Result<()> {
    if crate::sql::issue::get(pool, repo, number).await?.is_none() {
        bail!("issue #{number} not found");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::label::detach(&mut tx, repo, number, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn create_project_group(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    selection: &str,
) -> Result<()> {
    if selection != "single" && selection != "multi" {
        bail!("selection must be 'single' or 'multi'");
    }
    crate::sql::label::insert_project_group(pool, repo, name, selection).await
}

pub async fn create_project_label(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    group: Option<&str>,
) -> Result<()> {
    if let Some(group) = group {
        if !crate::sql::label::project_group_exists(pool, repo, group).await? {
            bail!("unknown label group {group:?}; create it first with `octa config label-group create --target project`");
        }
    }
    crate::sql::label::insert_project_label(pool, repo, name, group).await
}

pub async fn list_project_labels(pool: &SqlitePool, repo: i64) -> Result<Vec<Label>> {
    crate::sql::label::list_project_labels(pool, repo).await
}

pub async fn list_project_groups(pool: &SqlitePool, repo: i64) -> Result<Vec<LabelGroup>> {
    crate::sql::label::list_project_groups(pool, repo).await
}

pub async fn attach_project(
    pool: &SqlitePool,
    repo: i64,
    project_ref: &str,
    label: &str,
) -> Result<()> {
    let project = crate::app::project::resolve(pool, repo, project_ref).await?;
    let mut tx = pool.begin().await?;
    let group = crate::sql::label::project_label_group_tx(&mut tx, repo, label)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!("unknown label {label:?}; create it first with `octa config label create --target project`")
        })?;
    if let Some(group) = group {
        if crate::sql::label::project_group_selection_tx(&mut tx, repo, &group).await? == "single" {
            crate::sql::label::replace_project_single_group(&mut tx, repo, project.id, &group)
                .await?;
        }
    }
    crate::sql::label::attach_project(&mut tx, repo, project.id, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn detach_project(
    pool: &SqlitePool,
    repo: i64,
    project_ref: &str,
    label: &str,
) -> Result<()> {
    let project = crate::app::project::resolve(pool, repo, project_ref).await?;
    crate::sql::label::detach_project(pool, repo, project.id, label).await
}
