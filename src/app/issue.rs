//! Issue workflows: validation, policy, and composition of the SQL repository.

use crate::domain::issue::{
    Issue, IssueDetail, IssueListSelector, IssueState, LeaseOutcome, StateType,
};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

async fn require(pool: &SqlitePool, repository: i64, number: i64) -> Result<Issue> {
    crate::sql::issue::get(pool, repository, number)
        .await?
        .ok_or_else(|| anyhow!("issue #{number} not found"))
}

/// The state a verb moves an issue to when it is given no explicit target.
///
/// The schema keys the default by type, so a populated type always resolves to
/// exactly one state. Nothing coming back means the type has no states at all,
/// which only `in progress` is allowed to be.
async fn default_state(pool: &SqlitePool, state_type: StateType) -> Result<String> {
    crate::sql::issue::default_state(pool, state_type)
        .await?
        .ok_or_else(|| {
            anyhow!(
                "no {:?} state is configured; create one with `octa config issue state create <name> --type {:?}`",
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
    state_type: StateType,
    requested: Option<&str>,
) -> Result<String> {
    let Some(requested) = requested else {
        return default_state(pool, state_type).await;
    };
    let Some(state) = crate::sql::issue::get_state(pool, requested).await? else {
        // A verb narrowed to one type offers that type's states, not every
        // state: the wider set would name values this verb would then refuse.
        let available = crate::sql::issue::states_of_type(pool, state_type).await?;
        bail!(
            "unknown state {requested:?}; available {} states are: {}",
            state_type.as_str(),
            crate::domain::known_values(&available)
        );
    };
    if state.state_type != state_type {
        let available = crate::sql::issue::states_of_type(pool, state_type).await?;
        bail!(
            "state {requested:?} has type {:?}, not {:?}; available {} states are: {}",
            state.state_type.as_str(),
            state_type.as_str(),
            state_type.as_str(),
            crate::domain::known_values(&available)
        );
    }
    Ok(state.name)
}

pub(crate) struct ListQuery<'a> {
    pub selector: IssueListSelector,
    pub label: Option<&'a str>,
    pub project: Option<&'a str>,
    pub milestone: Option<&'a str>,
    pub related_to: Option<i64>,
    pub unblocked: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    repository: i64,
    title: &str,
    body: &str,
    state: Option<&str>,
    project: Option<&str>,
    milestone: Option<&str>,
    parent: Option<i64>,
) -> Result<i64> {
    // `open --as` is narrowed to its own type like every other verb. Capturing
    // an issue that is already underway or already resolved is two steps:
    // `open`, then `start` or `close`.
    let state = target_state(pool, StateType::Open, state).await?;
    let parent_issue = match parent {
        Some(number) => Some(require(pool, repository, number).await?),
        None => None,
    };
    let explicit_project = match project {
        Some(reference) => Some(crate::app::project::resolve(pool, repository, reference).await?),
        None => None,
    };
    if milestone.is_some() && explicit_project.is_none() {
        bail!("--milestone requires an explicit --project");
    }
    let inherited_project_id = explicit_project
        .as_ref()
        .map(|project| project.id)
        .or_else(|| {
            parent_issue
                .as_ref()?
                .project
                .as_ref()
                .map(|project| project.id)
        });
    let resolved_milestone = match (milestone, explicit_project.as_ref()) {
        (Some(reference), Some(project)) => {
            Some(crate::app::milestone::resolve(pool, repository, project.id, reference).await?)
        }
        _ => None,
    };
    let number = crate::sql::issue::insert(pool, repository, title, body, &state).await?;
    if let Some(project_id) = inherited_project_id {
        crate::sql::issue::set_project(pool, repository, number, project_id).await?;
    }
    if let Some(parent) = parent {
        crate::sql::issue::set_parent(pool, repository, number, parent).await?;
    }
    if let Some(milestone) = resolved_milestone {
        crate::sql::milestone::set_issue(
            pool,
            repository,
            number,
            milestone.project_id,
            milestone.id,
        )
        .await?;
    }
    Ok(number)
}

pub async fn list(
    pool: &SqlitePool,
    repository: Option<i64>,
    query: ListQuery<'_>,
) -> Result<Vec<Issue>> {
    let ListQuery {
        selector,
        label,
        project,
        milestone,
        related_to,
        unblocked,
    } = query;
    if repository.is_none()
        && (label.is_some()
            || project.is_some()
            || milestone.is_some()
            || related_to.is_some()
            || unblocked)
    {
        // States are global, so `--state` and `--state-type` stay usable across
        // repositories. The remaining filters name repository-scoped entities.
        bail!(
            "--label, --project, --milestone, --related-to and --unblocked need a single repository"
        );
    }
    if milestone.is_some() && project.is_none() {
        bail!("--milestone requires --project so names and ids resolve within a Project");
    }
    // A filter that names something undefined must say so. Matching nothing
    // silently reads as "no issues qualify", which is what sent a caller off
    // probing values against live records.
    if let IssueListSelector::States(names) = &selector {
        require_states(pool, names).await?;
    }
    if let Some(label) = label {
        crate::app::label::require_label(pool, label).await?;
    }
    let labelled = match label {
        Some(label) => Some(
            crate::sql::issue::labelled_numbers(
                pool,
                repository.expect("label requires a single repository"),
                label,
            )
            .await?,
        ),
        None => None,
    };
    let allowed = match unblocked {
        true => Some(
            unblocked_numbers(
                pool,
                repository.expect("unblocked requires a single repository"),
            )
            .await?,
        ),
        false => None,
    };
    let project_id = match project {
        Some(reference) => Some(
            crate::app::project::resolve(
                pool,
                repository.expect("project filter requires a single repository"),
                reference,
            )
            .await?
            .id,
        ),
        None => None,
    };
    let milestone_id = match (milestone, project_id) {
        (Some(reference), Some(project)) => Some(
            crate::app::milestone::resolve(
                pool,
                repository.expect("milestone filter requires a single repository"),
                project,
                reference,
            )
            .await?
            .id,
        ),
        _ => None,
    };
    let related_numbers = match related_to {
        Some(number) => {
            let repository = repository.expect("related filter requires a single repository");
            require(pool, repository, number).await?;
            Some(
                crate::sql::issue::related(pool, repository, number)
                    .await?
                    .into_iter()
                    .collect::<HashSet<_>>(),
            )
        }
        None => None,
    };
    let mut entries = crate::sql::issue::list_entries(pool, repository).await?;
    entries.retain(|entry| {
        selector.includes(entry.state_type, &entry.issue.state)
            && labelled
                .as_ref()
                .is_none_or(|set| set.contains(&entry.issue.number))
            && allowed
                .as_ref()
                .is_none_or(|set| set.contains(&entry.issue.number))
            && project_id.is_none_or(|id| entry.issue.project.as_ref().is_some_and(|p| p.id == id))
            && milestone_id
                .is_none_or(|id| entry.issue.milestone.as_ref().is_some_and(|m| m.id == id))
            && related_numbers
                .as_ref()
                .is_none_or(|set| set.contains(&entry.issue.number))
    });
    // Preserve the general list API's stable repository/issue-number order.
    entries.sort_by_key(|entry| (entry.issue.repository.clone(), entry.issue.number));
    Ok(entries.into_iter().map(|entry| entry.issue).collect())
}

async fn unblocked_numbers(pool: &SqlitePool, repository: i64) -> Result<HashSet<i64>> {
    let states: HashMap<i64, bool> = crate::sql::issue::closed_flags(pool, repository)
        .await?
        .into_iter()
        .collect();
    let blocked = crate::sql::issue::dependencies(pool, repository)
        .await?
        .into_iter()
        .filter(|(blocker, _)| !states.get(blocker).copied().unwrap_or(false))
        .map(|(_, blocked)| blocked)
        .collect::<HashSet<_>>();
    Ok(states
        .into_iter()
        .filter_map(|(number, closed)| (!closed && !blocked.contains(&number)).then_some(number))
        .collect())
}

pub async fn detail(pool: &SqlitePool, repository: i64, number: i64) -> Result<IssueDetail> {
    let issue = require(pool, repository, number).await?;
    Ok(IssueDetail {
        labels: crate::sql::issue::labels(pool, repository, number).await?,
        blocks: crate::sql::issue::blocks(pool, repository, number).await?,
        blocked_by: crate::sql::issue::blocked_by(pool, repository, number).await?,
        related: crate::sql::issue::related(pool, repository, number).await?,
        pull_requests: crate::sql::issue::linked_pull_requests(pool, repository, number).await?,
        parent: crate::sql::issue::parent(pool, repository, number).await?,
        sub_issues: crate::sql::issue::children(pool, repository, number).await?,
        comments: crate::sql::issue::comments(pool, repository, number).await?,
        issue,
    })
}

pub async fn add_relation(
    pool: &SqlitePool,
    repository: i64,
    a: i64,
    b: i64,
    lease: Option<&str>,
) -> Result<()> {
    if a == b {
        bail!("an issue cannot be related to itself");
    }
    require(pool, repository, a).await?;
    require(pool, repository, b).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, a, lease).await?;
    crate::sql::issue::insert_relation(&mut tx, repository, a, b).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_relation(
    pool: &SqlitePool,
    repository: i64,
    a: i64,
    b: i64,
    lease: Option<&str>,
) -> Result<()> {
    if a == b {
        bail!("an issue cannot be related to itself");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, a, lease).await?;
    crate::sql::issue::remove_relation(&mut tx, repository, a, b).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_project(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    reference: &str,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repository, number).await?;
    let project = crate::app::project::resolve(pool, repository, reference).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    if issue
        .project
        .as_ref()
        .is_some_and(|current| current.id == project.id)
    {
        tx.commit().await?;
        return Ok(());
    }
    if let Some(milestone) = &issue.milestone {
        bail!(
            "cannot move issue #{number} while milestone {:?} is assigned; clear the milestone first",
            milestone.name
        );
    }
    crate::sql::issue::set_project_tx(&mut tx, repository, number, project.id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_project(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repository, number).await?;
    if let Some(milestone) = &issue.milestone {
        bail!(
            "cannot clear issue project while milestone {:?} is assigned; clear the milestone first",
            milestone.name
        );
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::issue::clear_project_tx(&mut tx, repository, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_milestone(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    reference: &str,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repository, number).await?;
    let project = issue
        .project
        .ok_or_else(|| anyhow!("issue #{number} needs a project before assigning a milestone"))?;
    let milestone = crate::app::milestone::resolve(pool, repository, project.id, reference).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::milestone::set_issue_tx(&mut tx, repository, number, project.id, milestone.id)
        .await?;
    crate::sql::issue::touch_tx(&mut tx, repository, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_milestone(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repository, number).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::milestone::clear_issue_tx(&mut tx, repository, number).await?;
    crate::sql::issue::touch_tx(&mut tx, repository, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_parent(
    pool: &SqlitePool,
    repository: i64,
    child: i64,
    parent: i64,
    lease: Option<&str>,
) -> Result<()> {
    if child == parent {
        bail!("an issue cannot be its own parent");
    }
    let child_issue = require(pool, repository, child).await?;
    let parent_issue = require(pool, repository, parent).await?;
    let inherited_project = child_issue
        .project
        .is_none()
        .then(|| parent_issue.project.as_ref().map(|project| project.id))
        .flatten();
    crate::sql::issue::set_parent_transactional(
        pool,
        repository,
        child,
        parent,
        inherited_project,
        lease,
    )
    .await
    .map_err(|error| {
        if error.to_string().contains("issue parent cycle") {
            anyhow!("setting parent would create an issue parent cycle")
        } else {
            error
        }
    })?;
    Ok(())
}

pub async fn clear_parent(
    pool: &SqlitePool,
    repository: i64,
    child: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repository, child).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, child, lease).await?;
    crate::sql::issue::clear_parent_tx(&mut tx, repository, child).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn comment(pool: &SqlitePool, repository: i64, number: i64, body: &str) -> Result<()> {
    require(pool, repository, number).await?;
    crate::sql::issue::insert_comment(pool, repository, number, body).await?;
    crate::sql::issue::touch(pool, repository, number).await
}

/// Move an issue to any configured state. Backs `issue set --as`.
pub async fn set_state(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    state: &str,
    lease: Option<&str>,
) -> Result<()> {
    if !crate::sql::issue::state_exists(pool, state).await? {
        bail!(
            "unknown state {state:?}; available states are: {}",
            state_names(pool).await?
        );
    }
    move_to(pool, repository, number, state, lease).await
}

/// Move an issue to the `in progress` default.
///
/// `start` takes no `--as`: picking work up says nothing yet about where it
/// will land, so there is no second in-progress state to choose between.
pub async fn start(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<String> {
    let state = default_state(pool, StateType::InProgress).await?;
    move_to(pool, repository, number, &state, lease).await?;
    Ok(state)
}

/// Move an issue to a closed-type state.
pub async fn close(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    as_state: Option<&str>,
    lease: Option<&str>,
) -> Result<String> {
    let state = target_state(pool, StateType::Closed, as_state).await?;
    move_to(pool, repository, number, &state, lease).await?;
    Ok(state)
}

/// Move an issue back to an open-type state.
pub async fn reopen(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    as_state: Option<&str>,
    lease: Option<&str>,
) -> Result<String> {
    let state = target_state(pool, StateType::Open, as_state).await?;
    move_to(pool, repository, number, &state, lease).await?;
    Ok(state)
}

async fn move_to(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    state: &str,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repository, number).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::issue::update_state(&mut tx, repository, number, state).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn edit(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repository, number).await?;
    if title.is_none() && body.is_none() {
        bail!("nothing to update: pass --title and/or --body");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, number, lease).await?;
    crate::sql::issue::edit(&mut tx, repository, number, title, body).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn add_dependency(
    pool: &SqlitePool,
    repository: i64,
    leased_issue: i64,
    blocker: i64,
    blocked: i64,
    lease: Option<&str>,
) -> Result<()> {
    if blocker == blocked {
        bail!("an issue cannot block itself");
    }
    require(pool, repository, blocker).await?;
    require(pool, repository, blocked).await?;
    let mut tx =
        crate::sql::issue::begin_lease_mutation(pool, repository, leased_issue, lease).await?;
    crate::sql::issue::insert_dependency(&mut tx, repository, blocker, blocked).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_dependency(
    pool: &SqlitePool,
    repository: i64,
    leased_issue: i64,
    blocker: i64,
    blocked: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repository, blocker).await?;
    require(pool, repository, blocked).await?;
    let mut tx =
        crate::sql::issue::begin_lease_mutation(pool, repository, leased_issue, lease).await?;
    crate::sql::issue::remove_dependency(&mut tx, repository, blocker, blocked).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn lock(pool: &SqlitePool, repository: i64, number: i64) -> Result<LeaseOutcome> {
    require(pool, repository, number).await?;
    match crate::sql::issue::acquire_lease(pool, repository, number).await? {
        Some(lease) => Ok(LeaseOutcome::Acquired(lease)),
        None => Ok(LeaseOutcome::AlreadyLeased),
    }
}

pub async fn unlock(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    lease: Option<&str>,
    force: bool,
) -> Result<bool> {
    require(pool, repository, number).await?;
    crate::sql::issue::release_lease(pool, repository, number, lease, force).await
}

/// Reject `--state` names that are not configured.
async fn require_states(pool: &SqlitePool, names: &[String]) -> Result<()> {
    let configured = crate::sql::issue::list_states(pool).await?;
    for name in names {
        if !configured.iter().any(|state| &state.name == name) {
            let known =
                crate::domain::known_values(configured.iter().map(|state| state.name.as_str()));
            bail!("unknown state {name:?}; available states are: {known}");
        }
    }
    Ok(())
}

/// The available state names, formatted for an error that rejects one.
async fn state_names(pool: &SqlitePool) -> Result<String> {
    let configured = crate::sql::issue::list_states(pool).await?;
    Ok(crate::domain::known_values(
        configured.iter().map(|state| state.name.as_str()),
    ))
}

pub async fn list_states(pool: &SqlitePool) -> Result<Vec<IssueState>> {
    crate::sql::issue::list_states(pool).await
}

/// Create a state, returning whether it became its type's default unasked.
///
/// The first state of an empty type is always that type's default: a populated
/// type with no default would leave the type's verb with nowhere to go while an
/// obvious -- and only -- candidate sat right there.
pub async fn add_state(
    pool: &SqlitePool,
    name: &str,
    state_type: StateType,
    default: bool,
) -> Result<bool> {
    if crate::sql::issue::get_state(pool, name).await?.is_some() {
        bail!("a configured state named {name:?} already exists");
    }
    let promoted = !default && type_is_empty(pool, state_type).await?;
    crate::sql::issue::insert_state(pool, name, state_type, default || promoted).await?;
    Ok(promoted)
}

async fn type_is_empty(pool: &SqlitePool, state_type: StateType) -> Result<bool> {
    Ok(crate::sql::issue::states_of_type(pool, state_type)
        .await?
        .is_empty())
}

async fn require_state(pool: &SqlitePool, name: &str) -> Result<IssueState> {
    match crate::sql::issue::get_state(pool, name).await? {
        Some(state) => Ok(state),
        None => bail!(
            "unknown state {name:?}; available states are: {}",
            state_names(pool).await?
        ),
    }
}

/// Types that must always have at least one state.
///
/// Every issue has to be able to start and to end, so emptying `open` or
/// `closed` would leave the verbs with nowhere to go. `in progress` may be
/// empty: a workflow that never distinguishes picked-up work is coherent.
const REQUIRED_TYPES: [StateType; 2] = [StateType::Open, StateType::Closed];

/// Reject a change that would leave a required type with no state.
async fn require_type_stays_populated(
    pool: &SqlitePool,
    state_type: StateType,
    leaving: &str,
) -> Result<()> {
    if !REQUIRED_TYPES.contains(&state_type) {
        return Ok(());
    }
    let remaining = crate::sql::issue::states_of_type(pool, state_type)
        .await?
        .into_iter()
        .filter(|name| name != leaving)
        .count();
    if remaining == 0 {
        bail!(
            "{leaving:?} is the only {:?} state; create another one first",
            state_type.as_str()
        );
    }
    Ok(())
}

/// Reject dropping a type's default while that type has somewhere else to
/// point.
///
/// The last state of an optional type may go with its flag: emptying the type
/// is the whole point. What must not happen is a populated type left with no
/// default, which would break the verb that resolves to it.
async fn require_default_can_be_released(pool: &SqlitePool, state: &IssueState) -> Result<()> {
    if !state.is_default {
        return Ok(());
    }
    let siblings = crate::sql::issue::states_of_type(pool, state.state_type)
        .await?
        .into_iter()
        .filter(|name| name != &state.name)
        .count();
    if siblings > 0 {
        bail!(
            "{:?} is the default {:?} state; run `octa config issue state set <name> --default` on another {:?} state first",
            state.name,
            state.state_type.as_str(),
            state.state_type.as_str()
        );
    }
    Ok(())
}

/// Update a state's name, or make it its type's default.
///
/// Renaming repoints every issue in the state, so no issue is left pointing at
/// a name that no longer exists. `default` needs no type argument: a state
/// already carries exactly one type, so naming it again could only contradict
/// it.
pub async fn set_state_config(
    pool: &SqlitePool,
    name: &str,
    new_name: Option<&str>,
    default: bool,
) -> Result<()> {
    let state = require_state(pool, name).await?;
    if new_name.is_none() && !default {
        bail!("nothing to update; pass --name or --default");
    }

    let mut name = name.to_string();
    if let Some(new_name) = new_name {
        if new_name != name {
            if crate::sql::issue::get_state(pool, new_name)
                .await?
                .is_some()
            {
                bail!("a configured state named {new_name:?} already exists");
            }
            crate::sql::issue::rename_state(pool, &name, new_name).await?;
            name = new_name.to_string();
        }
    }
    if default {
        crate::sql::issue::set_default_state(pool, &name, state.state_type).await?;
    }
    Ok(())
}

/// Delete a state, moving any issues that reference it to `move_to`.
pub async fn delete_state(pool: &SqlitePool, name: &str, move_to: Option<&str>) -> Result<i64> {
    let state = require_state(pool, name).await?;
    require_type_stays_populated(pool, state.state_type, name).await?;
    require_default_can_be_released(pool, &state).await?;
    let move_to = match move_to {
        Some(target) if target == name => bail!("--move-to must name a different state"),
        Some(target) => {
            require_state(pool, target).await?;
            Some(target)
        }
        None => None,
    };
    let affected = crate::sql::issue::count_issues_in_state(pool, name).await?;
    if affected > 0 && move_to.is_none() {
        bail!(
            "{affected} issue(s) across all repositories are in {name:?}; pass --move-to <state> to move them"
        );
    }
    crate::sql::issue::delete_state(pool, name, move_to).await?;
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::set_parent;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn fixture_pool() -> sqlx::SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_init.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!("INSERT INTO repositories (id, path, name) VALUES (1, 'fixture', 'fixture')")
            .execute(&pool)
            .await
            .unwrap();
        // `issues.state` and `projects.state` reference their configured state
        // tables, so the raw inserts below need the state sets they name.
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        crate::sql::project::seed_default_states(&pool)
            .await
            .unwrap();
        pool
    }

    #[tokio::test]
    async fn failed_parent_cycle_rolls_back_project_inheritance() {
        let pool = fixture_pool().await;
        for number in [1_i64, 2] {
            let title = format!("issue {number}");
            sqlx::query!(
                "INSERT INTO issues (repository_id, number, title, state) VALUES (1, ?, ?, 'open')",
                number,
                title
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query!(
            "INSERT INTO projects (repository_id, id, name, state) VALUES (1, 1, 'parent project', 'open')"
        )
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO issue_projects (repository_id, issue_number, project_id) VALUES (1, 2, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Raw fixture deliberately models legacy/inconsistent data: #2 is
        // already below #1, but only #2 has a project.
        sqlx::query!(
            "INSERT INTO issue_parents (repository_id, child_number, parent_number) VALUES (1, 2, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let lease = crate::sql::issue::acquire_lease(&pool, 1, 1)
            .await
            .unwrap()
            .unwrap();
        let error = set_parent(&pool, 1, 1, 2, Some(&lease)).await.unwrap_err();
        assert!(error.to_string().contains("cycle"));
        let inherited = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM issue_projects WHERE repository_id = 1 AND issue_number = 1"
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(inherited, 0, "failed relation leaked inherited project");
    }

    #[tokio::test]
    async fn milestone_schema_rejects_cross_project_issue_assignment() {
        let pool = fixture_pool().await;
        sqlx::query!(
            "INSERT INTO issues (repository_id, number, title, state) VALUES (1, 1, 'issue', 'open')",
        )
        .execute(&pool)
        .await
        .unwrap();
        for id in [1_i64, 2] {
            let name = format!("project {id}");
            sqlx::query!(
                "INSERT INTO projects (repository_id, id, name, state) VALUES (1, ?, ?, 'open')",
                id,
                name
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query!(
            "INSERT INTO issue_projects (repository_id, issue_number, project_id) VALUES (1, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO project_milestones \
             (repository_id, project_id, id, position, name) VALUES (1, 2, 1, 0, 'other')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = sqlx::query!(
            "INSERT INTO issue_milestones \
             (repository_id, issue_number, project_id, milestone_id) VALUES (1, 1, 2, 1)",
        )
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("FOREIGN KEY constraint failed"),
            "{error}"
        );
    }
}
