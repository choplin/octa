use crate::domain::label::{Label, LabelGroup, LabelSelection};
use anyhow::{bail, Result};
use sqlx::SqlitePool;
pub async fn create_group(pool: &SqlitePool, name: &str, selection: &str) -> Result<()> {
    let selection = LabelSelection::parse(selection)?;
    crate::sql::label::insert_group(pool, name, selection.as_str()).await
}

pub async fn create(pool: &SqlitePool, name: &str, group: Option<&str>) -> Result<()> {
    if let Some(group) = group {
        if !crate::sql::label::group_exists(pool, group).await? {
            bail!(
                "unknown label group {group:?}; available label groups are: {}",
                group_names(pool).await?
            );
        }
    }
    crate::sql::label::insert(pool, name, group).await
}

pub async fn list(pool: &SqlitePool) -> Result<Vec<Label>> {
    crate::sql::label::list(pool).await
}

pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<LabelGroup>> {
    crate::sql::label::list_groups(pool).await
}

/// Reject a label name that is not defined.
///
/// Filtering on an undefined label used to match nothing, which reads exactly
/// like "no issue carries it" — a typo and an empty result were indissoluble.
/// Detaching stays idempotent; only reads that would otherwise answer silently
/// go through here.
pub async fn require_label(pool: &SqlitePool, label: &str) -> Result<()> {
    let defined = crate::sql::label::list(pool).await?;
    if defined.iter().any(|candidate| candidate.name == label) {
        return Ok(());
    }
    bail!(
        "unknown label {label:?}; available labels are: {}",
        crate::domain::known_values(defined.iter().map(|candidate| candidate.name.as_str()))
    )
}

/// The available Issue label names, formatted for an error that rejects one.
async fn label_names(pool: &SqlitePool) -> Result<String> {
    let defined = crate::sql::label::list(pool).await?;
    Ok(crate::domain::known_values(
        defined.iter().map(|label| label.name.as_str()),
    ))
}

/// The Project-label counterpart of `label_names`.
async fn project_label_names(pool: &SqlitePool) -> Result<String> {
    let defined = crate::sql::label::list_project_labels(pool).await?;
    Ok(crate::domain::known_values(
        defined.iter().map(|label| label.name.as_str()),
    ))
}

/// The available Issue label group names, formatted for an error that rejects one.
async fn group_names(pool: &SqlitePool) -> Result<String> {
    let defined = crate::sql::label::list_groups(pool).await?;
    Ok(crate::domain::known_values(
        defined.iter().map(|group| group.name.as_str()),
    ))
}

/// The Project-label-group counterpart of `group_names`.
async fn project_group_names(pool: &SqlitePool) -> Result<String> {
    let defined = crate::sql::label::list_project_groups(pool).await?;
    Ok(crate::domain::known_values(
        defined.iter().map(|group| group.name.as_str()),
    ))
}

pub async fn attach(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    label: &str,
    lease: Option<&str>,
) -> Result<()> {
    if crate::sql::issue::get(pool, repository, number)
        .await?
        .is_none()
    {
        bail!("issue #{number} not found");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    let Some(group) = crate::sql::label::label_group_tx(&mut tx, label).await? else {
        bail!(
            "unknown label {label:?}; available labels are: {}",
            label_names(pool).await?
        );
    };
    if let Some(group) = group {
        if crate::sql::label::group_selection_tx(&mut tx, &group).await? == "single" {
            crate::sql::label::replace_single_group(&mut tx, repository, number, &group).await?;
        }
    }
    crate::sql::label::attach(&mut tx, repository, number, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn detach(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    label: &str,
    lease: Option<&str>,
) -> Result<()> {
    if crate::sql::issue::get(pool, repository, number)
        .await?
        .is_none()
    {
        bail!("issue #{number} not found");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::label::detach(&mut tx, repository, number, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn create_project_group(pool: &SqlitePool, name: &str, selection: &str) -> Result<()> {
    let selection = LabelSelection::parse(selection)?;
    crate::sql::label::insert_project_group(pool, name, selection.as_str()).await
}

pub async fn create_project_label(
    pool: &SqlitePool,
    name: &str,
    group: Option<&str>,
) -> Result<()> {
    if let Some(group) = group {
        if !crate::sql::label::project_group_exists(pool, group).await? {
            bail!(
                "unknown label group {group:?}; available label groups are: {}",
                project_group_names(pool).await?
            );
        }
    }
    crate::sql::label::insert_project_label(pool, name, group).await
}

pub async fn list_project_labels(pool: &SqlitePool) -> Result<Vec<Label>> {
    crate::sql::label::list_project_labels(pool).await
}

pub async fn list_project_groups(pool: &SqlitePool) -> Result<Vec<LabelGroup>> {
    crate::sql::label::list_project_groups(pool).await
}

pub async fn attach_project(
    pool: &SqlitePool,
    repository: i64,
    project_ref: &str,
    label: &str,
) -> Result<()> {
    let project = crate::app::project::resolve(pool, repository, project_ref).await?;
    let mut tx = pool.begin().await?;
    let Some(group) = crate::sql::label::project_label_group_tx(&mut tx, label).await? else {
        bail!(
            "unknown label {label:?}; available labels are: {}",
            project_label_names(pool).await?
        );
    };
    if let Some(group) = group {
        if crate::sql::label::project_group_selection_tx(&mut tx, &group).await? == "single" {
            crate::sql::label::replace_project_single_group(
                &mut tx, repository, project.id, &group,
            )
            .await?;
        }
    }
    crate::sql::label::attach_project(&mut tx, repository, project.id, label).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn detach_project(
    pool: &SqlitePool,
    repository: i64,
    project_ref: &str,
    label: &str,
) -> Result<()> {
    let project = crate::app::project::resolve(pool, repository, project_ref).await?;
    crate::sql::label::detach_project(pool, repository, project.id, label).await
}
