use crate::domain::milestone::ProjectMilestone;
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;

pub async fn resolve(
    pool: &SqlitePool,
    repo: i64,
    project: i64,
    reference: &str,
) -> Result<ProjectMilestone> {
    let milestone = match reference.parse::<i64>() {
        Ok(id) => crate::sql::milestone::get_by_id(pool, repo, project, id).await?,
        Err(_) => crate::sql::milestone::get_by_name(pool, repo, project, reference).await?,
    };
    milestone.ok_or_else(|| anyhow!("milestone {reference:?} not found in project"))
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("milestone name cannot be empty");
    }
    if name.parse::<i64>().is_ok() {
        bail!("milestone name cannot be numeric because numeric references identify milestone ids");
    }
    Ok(())
}

fn validate_position(position: i64) -> Result<i64> {
    if position < 0 {
        bail!("milestone position cannot be negative");
    }
    Ok(position)
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    repo: i64,
    project_reference: &str,
    name: &str,
    description: &str,
    status: &str,
    position: Option<i64>,
    start_date: Option<&str>,
    target_date: Option<&str>,
) -> Result<i64> {
    validate_name(name)?;
    let project = crate::app::project::resolve(pool, repo, project_reference).await?;
    crate::sql::milestone::insert(
        pool,
        repo,
        project.id,
        name,
        description,
        status,
        position.map(validate_position).transpose()?,
        start_date,
        target_date,
    )
    .await
}

pub async fn list(
    pool: &SqlitePool,
    repo: i64,
    project_reference: &str,
) -> Result<Vec<ProjectMilestone>> {
    let project = crate::app::project::resolve(pool, repo, project_reference).await?;
    crate::sql::milestone::list(pool, repo, project.id).await
}

pub async fn show(
    pool: &SqlitePool,
    repo: i64,
    project_reference: &str,
    reference: &str,
) -> Result<ProjectMilestone> {
    let project = crate::app::project::resolve(pool, repo, project_reference).await?;
    resolve(pool, repo, project.id, reference).await
}

#[allow(clippy::too_many_arguments)]
pub async fn edit(
    pool: &SqlitePool,
    repo: i64,
    project_reference: &str,
    reference: &str,
    name: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
    position: Option<i64>,
    start_date: Option<&str>,
    target_date: Option<&str>,
    clear_start_date: bool,
    clear_target_date: bool,
) -> Result<()> {
    if name.is_none()
        && description.is_none()
        && status.is_none()
        && position.is_none()
        && start_date.is_none()
        && target_date.is_none()
        && !clear_start_date
        && !clear_target_date
    {
        bail!("nothing to update: pass milestone metadata or a date-clear flag");
    }
    if start_date.is_some() && clear_start_date {
        bail!("--start-date conflicts with --clear-start-date");
    }
    if target_date.is_some() && clear_target_date {
        bail!("--target-date conflicts with --clear-target-date");
    }
    if let Some(name) = name {
        validate_name(name)?;
    }
    let project = crate::app::project::resolve(pool, repo, project_reference).await?;
    let milestone = resolve(pool, repo, project.id, reference).await?;
    crate::sql::milestone::update(
        pool,
        repo,
        project.id,
        milestone.id,
        name,
        description,
        status,
        position.map(validate_position).transpose()?,
        start_date,
        target_date,
        clear_start_date,
        clear_target_date,
    )
    .await
}
