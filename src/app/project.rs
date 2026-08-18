use crate::domain::project::{
    Project, ProjectDetail, ProjectOverview, ProjectState, ProjectStateType,
};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;

async fn default_state(pool: &SqlitePool, state_type: ProjectStateType) -> Result<String> {
    crate::sql::project::default_state(pool, state_type)
        .await?
        .ok_or_else(|| {
            anyhow!(
                "no {:?} project state is configured; create one with `octa config project state create <name> --type {:?}`",
                state_type.as_str(),
                state_type.as_str()
            )
        })
}

/// Resolve a verb's target state: the explicit `--as` value or the type default.
///
/// `--as` accepts only states of the verb's own type. A verb whose name states
/// the outcome must not be a back door into an unrelated state.
async fn target_state(
    pool: &SqlitePool,
    state_type: ProjectStateType,
    requested: Option<&str>,
) -> Result<String> {
    let Some(requested) = requested else {
        return default_state(pool, state_type).await;
    };
    let state = crate::sql::project::get_state(pool, requested)
        .await?
        .ok_or_else(|| anyhow!("no configured project state named {requested:?}"))?;
    if state.state_type != state_type {
        let available = crate::sql::project::states_of_type(pool, state_type).await?;
        bail!(
            "project state {requested:?} has type {:?}, not {:?}; available: {}",
            state.state_type.as_str(),
            state_type.as_str(),
            available.join(", ")
        );
    }
    Ok(state.name)
}

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
    state: Option<&str>,
) -> Result<i64> {
    validate_name(name)?;
    let state = target_state(pool, ProjectStateType::Open, state).await?;
    crate::sql::project::insert(pool, repo, name, summary, description, &state).await
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

/// Move a project to any configured state. Backs `project set --as`.
pub async fn set_state(pool: &SqlitePool, repo: i64, reference: &str, state: &str) -> Result<()> {
    if !crate::sql::project::state_exists(pool, state).await? {
        bail!(
            "unknown project state {state:?}; create it first with `octa config project state create`"
        );
    }
    let project = resolve(pool, repo, reference).await?;
    crate::sql::project::set_state(pool, repo, project.id, state).await
}

/// Move a project to a closed-type state.
pub async fn close(
    pool: &SqlitePool,
    repo: i64,
    reference: &str,
    as_state: Option<&str>,
) -> Result<String> {
    let state = target_state(pool, ProjectStateType::Closed, as_state).await?;
    let project = resolve(pool, repo, reference).await?;
    crate::sql::project::set_state(pool, repo, project.id, &state).await?;
    Ok(state)
}

/// Move a project back to an open-type state.
pub async fn reopen(
    pool: &SqlitePool,
    repo: i64,
    reference: &str,
    as_state: Option<&str>,
) -> Result<String> {
    let state = target_state(pool, ProjectStateType::Open, as_state).await?;
    let project = resolve(pool, repo, reference).await?;
    crate::sql::project::set_state(pool, repo, project.id, &state).await?;
    Ok(state)
}

pub async fn list_states(pool: &SqlitePool) -> Result<Vec<ProjectState>> {
    crate::sql::project::list_states(pool).await
}

/// Create a state, returning whether it became its type's default unasked.
///
/// The first state of an empty type is always that type's default: a populated
/// type with no default would leave the type's verb with nowhere to go while an
/// obvious -- and only -- candidate sat right there.
pub async fn add_state(
    pool: &SqlitePool,
    name: &str,
    state_type: ProjectStateType,
    default: bool,
) -> Result<bool> {
    if crate::sql::project::get_state(pool, name).await?.is_some() {
        bail!("a configured project state named {name:?} already exists");
    }
    let promoted = !default && type_is_empty(pool, state_type).await?;
    crate::sql::project::insert_state(pool, name, state_type, default || promoted).await?;
    Ok(promoted)
}

async fn type_is_empty(pool: &SqlitePool, state_type: ProjectStateType) -> Result<bool> {
    Ok(crate::sql::project::states_of_type(pool, state_type)
        .await?
        .is_empty())
}

async fn require_state(pool: &SqlitePool, name: &str) -> Result<ProjectState> {
    crate::sql::project::get_state(pool, name)
        .await?
        .ok_or_else(|| anyhow!("no configured project state named {name:?}"))
}

/// Reject a change that would leave a type with no state.
///
/// Every project has to be able to start and to end, and the axis has only
/// those two types, so neither may be emptied.
async fn require_type_stays_populated(
    pool: &SqlitePool,
    state_type: ProjectStateType,
    leaving: &str,
) -> Result<()> {
    let remaining = crate::sql::project::states_of_type(pool, state_type)
        .await?
        .into_iter()
        .filter(|name| name != leaving)
        .count();
    if remaining == 0 {
        bail!(
            "{leaving:?} is the only {:?} project state; create another one first",
            state_type.as_str()
        );
    }
    Ok(())
}

/// Reject dropping a type's default while that type has somewhere else to
/// point.
async fn require_default_can_be_released(pool: &SqlitePool, state: &ProjectState) -> Result<()> {
    if !state.is_default {
        return Ok(());
    }
    let siblings = crate::sql::project::states_of_type(pool, state.state_type)
        .await?
        .into_iter()
        .filter(|name| name != &state.name)
        .count();
    if siblings > 0 {
        bail!(
            "{:?} is the default {:?} project state; run `octa config project state set <name> --default` on another {:?} state first",
            state.name,
            state.state_type.as_str(),
            state.state_type.as_str()
        );
    }
    Ok(())
}

/// Update a state's name or type, or make it its type's default. Returns
/// whether it became a default unasked.
///
/// Renaming repoints every project in the state, so no project is left pointing
/// at a name that no longer exists. `default` needs no type argument: a state
/// already carries exactly one type, so naming it again could only contradict
/// it. When both are given the state is retyped first, then made the default of
/// the type it ends up in.
pub async fn set_state_config(
    pool: &SqlitePool,
    name: &str,
    new_name: Option<&str>,
    state_type: Option<ProjectStateType>,
    default: bool,
) -> Result<bool> {
    let state = require_state(pool, name).await?;
    if new_name.is_none() && state_type.is_none() && !default {
        bail!("nothing to update; pass --name, --type, or --default");
    }

    // Read before anything moves: once the state has been retyped it is itself
    // a member of the destination type.
    let mut promoted = false;
    if let Some(state_type) = state_type {
        if state_type != state.state_type {
            require_type_stays_populated(pool, state.state_type, name).await?;
            require_default_can_be_released(pool, &state).await?;
            promoted = !default && type_is_empty(pool, state_type).await?;
        }
    }

    let mut name = name.to_string();
    if let Some(new_name) = new_name {
        if new_name != name {
            if crate::sql::project::get_state(pool, new_name)
                .await?
                .is_some()
            {
                bail!("a configured project state named {new_name:?} already exists");
            }
            crate::sql::project::rename_state(pool, &name, new_name).await?;
            name = new_name.to_string();
        }
    }
    if let Some(state_type) = state_type {
        if state_type != state.state_type {
            crate::sql::project::update_state_type(pool, &name, state_type).await?;
        }
    }
    if default || promoted {
        let state_type = state_type.unwrap_or(state.state_type);
        crate::sql::project::set_default_state(pool, &name, state_type).await?;
    }
    Ok(promoted)
}

/// Delete a state, moving any projects that reference it to `move_to`.
pub async fn delete_state(pool: &SqlitePool, name: &str, move_to: Option<&str>) -> Result<i64> {
    let state = require_state(pool, name).await?;
    require_type_stays_populated(pool, state.state_type, name).await?;
    require_default_can_be_released(pool, &state).await?;
    let move_to = match move_to {
        Some(target) if target == name => bail!("--move-to must name a different project state"),
        Some(target) => {
            require_state(pool, target).await?;
            Some(target)
        }
        None => None,
    };
    let affected = crate::sql::project::count_projects_in_state(pool, name).await?;
    if affected > 0 && move_to.is_none() {
        bail!(
            "{affected} project(s) across all repositories are in {name:?}; pass --move-to <state> to move them"
        );
    }
    crate::sql::project::delete_state(pool, name, move_to).await?;
    Ok(affected)
}
