use crate::domain::project::{Project, ProjectDetail, ProjectOverview};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;

pub async fn resolve(pool: &SqlitePool, repo: i64, reference: &str) -> Result<Project> {
    let project = match reference.parse::<i64>() {
        Ok(id) => crate::sql::project::get_by_id(pool, repo, id).await?,
        Err(_) => crate::sql::project::get_by_name(pool, repo, reference).await?,
    };
    project.ok_or_else(|| anyhow!("project {reference:?} not found"))
}

pub async fn create(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    summary: &str,
    description: &str,
    state: &str,
    terminal: bool,
) -> Result<i64> {
    validate_name(name)?;
    crate::sql::project::insert(pool, repo, name, summary, description, state, terminal).await
}

pub async fn list(
    pool: &SqlitePool,
    repo: Option<i64>,
    active_only: bool,
) -> Result<Vec<ProjectOverview>> {
    let projects = crate::sql::project::list(pool, repo, active_only).await?;
    let mut overview = Vec::with_capacity(projects.len());
    for project in projects {
        let tally = crate::sql::project::tally(pool, project.repo_id, project.id).await?;
        let milestones = crate::sql::milestone::list(pool, project.repo_id, project.id).await?;
        overview.push(ProjectOverview {
            project,
            tally,
            milestones,
        });
    }
    Ok(overview)
}

pub async fn detail(pool: &SqlitePool, repo: i64, reference: &str) -> Result<ProjectDetail> {
    let project = resolve(pool, repo, reference).await?;
    Ok(ProjectDetail {
        tally: crate::sql::project::tally(pool, repo, project.id).await?,
        issue_numbers: crate::sql::project::issue_numbers(pool, repo, project.id).await?,
        milestones: crate::sql::milestone::list(pool, repo, project.id).await?,
        labels: crate::sql::label::labels_for_project(pool, repo, project.id).await?,
        project,
    })
}

fn validate_name(name: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("project name cannot be empty");
    }
    if name.parse::<i64>().is_ok() {
        bail!("project name cannot be numeric because numeric references identify project ids");
    }
    Ok(())
}

pub async fn edit(
    pool: &SqlitePool,
    repo: i64,
    reference: &str,
    name: Option<&str>,
    summary: Option<&str>,
    description: Option<&str>,
) -> Result<()> {
    if name.is_none() && summary.is_none() && description.is_none() {
        bail!("nothing to update: pass --name, --summary and/or --description");
    }
    let project = resolve(pool, repo, reference).await?;
    if let Some(name) = name {
        validate_name(name)?;
    }
    crate::sql::project::update(pool, repo, project.id, name, summary, description).await
}

pub async fn set_state(
    pool: &SqlitePool,
    repo: i64,
    reference: &str,
    state: &str,
    terminal: bool,
) -> Result<()> {
    let project = resolve(pool, repo, reference).await?;
    crate::sql::project::set_state(pool, repo, project.id, state, terminal).await
}
