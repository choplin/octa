//! Issue workflows: validation, policy, and composition of the SQL repository.

use crate::domain::{
    issue::{Issue, IssueDetail, IssueState, LeaseOutcome},
    StateFilter,
};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

async fn require(pool: &SqlitePool, repo: i64, number: i64) -> Result<Issue> {
    crate::sql::issue::get(pool, repo, number)
        .await?
        .ok_or_else(|| anyhow!("issue #{number} not found"))
}
async fn starting(pool: &SqlitePool) -> Result<String> {
    let mut candidates = crate::sql::issue::starting_states(pool).await?;
    match candidates.len() {
        1 => Ok(candidates.remove(0)),
        0 => bail!("no starting state configured; run `octa config state set-default <name>`"),
        _ => bail!(
            "{} states are flagged as starting ({}); run `octa config state set-default <name>` to pick one",
            candidates.len(),
            candidates.join(", ")
        ),
    }
}
pub(crate) struct ListQuery<'a> {
    pub filter: StateFilter,
    pub state_name: Option<&'a str>,
    pub label: Option<&'a str>,
    pub project: Option<&'a str>,
    pub milestone: Option<&'a str>,
    pub related_to: Option<i64>,
    pub unblocked: bool,
}

#[allow(clippy::too_many_arguments)]
pub async fn create(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    state: Option<&str>,
    project: Option<&str>,
    milestone: Option<&str>,
    parent: Option<i64>,
) -> Result<i64> {
    let state = match state {
        Some(state) => {
            if !crate::sql::issue::state_exists(pool, state).await? {
                bail!("unknown state {state:?}; create it first with `octa config state create`");
            }
            state.to_string()
        }
        None => starting(pool).await?,
    };
    let parent_issue = match parent {
        Some(number) => Some(require(pool, repo, number).await?),
        None => None,
    };
    let explicit_project = match project {
        Some(reference) => Some(crate::app::project::resolve(pool, repo, reference).await?),
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
            Some(crate::app::milestone::resolve(pool, repo, project.id, reference).await?)
        }
        _ => None,
    };
    let number = crate::sql::issue::insert(pool, repo, title, body, &state).await?;
    if let Some(project_id) = inherited_project_id {
        crate::sql::issue::set_project(pool, repo, number, project_id).await?;
    }
    if let Some(parent) = parent {
        crate::sql::issue::set_parent(pool, repo, number, parent).await?;
    }
    if let Some(milestone) = resolved_milestone {
        crate::sql::milestone::set_issue(pool, repo, number, milestone.project_id, milestone.id)
            .await?;
    }
    Ok(number)
}

pub async fn list(
    pool: &SqlitePool,
    repo: Option<i64>,
    query: ListQuery<'_>,
) -> Result<Vec<Issue>> {
    let ListQuery {
        filter,
        state_name,
        label,
        project,
        milestone,
        related_to,
        unblocked,
    } = query;
    if repo.is_none()
        && (state_name.is_some()
            || label.is_some()
            || project.is_some()
            || milestone.is_some()
            || related_to.is_some()
            || unblocked)
    {
        bail!(
            "--state <name>, --label, --project, --milestone, --related-to and --unblocked need a single repository"
        );
    }
    if milestone.is_some() && project.is_none() {
        bail!("--milestone requires --project so names and ids resolve within a Project");
    }
    let labelled = match label {
        Some(label) => Some(
            crate::sql::issue::labelled_numbers(
                pool,
                repo.expect("label requires a single repository"),
                label,
            )
            .await?,
        ),
        None => None,
    };
    let allowed = match unblocked {
        true => Some(
            unblocked_numbers(pool, repo.expect("unblocked requires a single repository")).await?,
        ),
        false => None,
    };
    let project_id = match project {
        Some(reference) => Some(
            crate::app::project::resolve(
                pool,
                repo.expect("project filter requires a single repository"),
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
                repo.expect("milestone filter requires a single repository"),
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
            let repo = repo.expect("related filter requires a single repository");
            require(pool, repo, number).await?;
            Some(
                crate::sql::issue::related(pool, repo, number)
                    .await?
                    .into_iter()
                    .collect::<HashSet<_>>(),
            )
        }
        None => None,
    };
    let mut entries = crate::sql::issue::list_entries(pool, repo).await?;
    entries.retain(|entry| {
        filter.includes(entry.is_terminal)
            && state_name.is_none_or(|state| entry.issue.state == state)
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
    entries.sort_by_key(|entry| (entry.issue.repo.clone(), entry.issue.number));
    Ok(entries.into_iter().map(|entry| entry.issue).collect())
}

async fn unblocked_numbers(pool: &SqlitePool, repo: i64) -> Result<HashSet<i64>> {
    let states: HashMap<i64, bool> = crate::sql::issue::state_flags(pool, repo)
        .await?
        .into_iter()
        .collect();
    let blocked = crate::sql::issue::dependencies(pool, repo)
        .await?
        .into_iter()
        .filter(|(blocker, _)| !states.get(blocker).copied().unwrap_or(false))
        .map(|(_, blocked)| blocked)
        .collect::<HashSet<_>>();
    Ok(states
        .into_iter()
        .filter_map(|(number, terminal)| {
            (!terminal && !blocked.contains(&number)).then_some(number)
        })
        .collect())
}

pub async fn detail(pool: &SqlitePool, repo: i64, number: i64) -> Result<IssueDetail> {
    let issue = require(pool, repo, number).await?;
    Ok(IssueDetail {
        labels: crate::sql::issue::labels(pool, repo, number).await?,
        blocks: crate::sql::issue::blocks(pool, repo, number).await?,
        blocked_by: crate::sql::issue::blocked_by(pool, repo, number).await?,
        related: crate::sql::issue::related(pool, repo, number).await?,
        pull_requests: crate::sql::issue::linked_prs(pool, repo, number).await?,
        parent: crate::sql::issue::parent(pool, repo, number).await?,
        sub_issues: crate::sql::issue::children(pool, repo, number).await?,
        comments: crate::sql::issue::comments(pool, repo, number).await?,
        issue,
    })
}

pub async fn add_relation(
    pool: &SqlitePool,
    repo: i64,
    a: i64,
    b: i64,
    lease: Option<&str>,
) -> Result<()> {
    if a == b {
        bail!("an issue cannot be related to itself");
    }
    require(pool, repo, a).await?;
    require(pool, repo, b).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, a, lease).await?;
    crate::sql::issue::insert_relation(&mut tx, repo, a, b).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_relation(
    pool: &SqlitePool,
    repo: i64,
    a: i64,
    b: i64,
    lease: Option<&str>,
) -> Result<()> {
    if a == b {
        bail!("an issue cannot be related to itself");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, a, lease).await?;
    crate::sql::issue::remove_relation(&mut tx, repo, a, b).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_project(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    reference: &str,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repo, number).await?;
    let project = crate::app::project::resolve(pool, repo, reference).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
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
    crate::sql::issue::set_project_tx(&mut tx, repo, number, project.id).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_project(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repo, number).await?;
    if let Some(milestone) = &issue.milestone {
        bail!(
            "cannot clear issue project while milestone {:?} is assigned; clear the milestone first",
            milestone.name
        );
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::issue::clear_project_tx(&mut tx, repo, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_milestone(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    reference: &str,
    lease: Option<&str>,
) -> Result<()> {
    let issue = require(pool, repo, number).await?;
    let project = issue
        .project
        .ok_or_else(|| anyhow!("issue #{number} needs a project before assigning a milestone"))?;
    let milestone = crate::app::milestone::resolve(pool, repo, project.id, reference).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::milestone::set_issue_tx(&mut tx, repo, number, project.id, milestone.id).await?;
    crate::sql::issue::touch_tx(&mut tx, repo, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_milestone(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repo, number).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::milestone::clear_issue_tx(&mut tx, repo, number).await?;
    crate::sql::issue::touch_tx(&mut tx, repo, number).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn set_parent(
    pool: &SqlitePool,
    repo: i64,
    child: i64,
    parent: i64,
    lease: Option<&str>,
) -> Result<()> {
    if child == parent {
        bail!("an issue cannot be its own parent");
    }
    let child_issue = require(pool, repo, child).await?;
    let parent_issue = require(pool, repo, parent).await?;
    let inherited_project = child_issue
        .project
        .is_none()
        .then(|| parent_issue.project.as_ref().map(|project| project.id))
        .flatten();
    crate::sql::issue::set_parent_transactional(
        pool,
        repo,
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
    repo: i64,
    child: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repo, child).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, child, lease).await?;
    crate::sql::issue::clear_parent_tx(&mut tx, repo, child).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn comment(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    require(pool, repo, number).await?;
    crate::sql::issue::insert_comment(pool, repo, number, body).await?;
    crate::sql::issue::touch(pool, repo, number).await
}

pub async fn set_state(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    state: &str,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repo, number).await?;
    if !crate::sql::issue::state_exists(pool, state).await? {
        bail!("unknown state {state:?}; create it first with `octa config state create`");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::issue::update_state(&mut tx, repo, number, state).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn edit(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repo, number).await?;
    if title.is_none() && body.is_none() {
        bail!("nothing to update: pass --title and/or --body");
    }
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, number, lease).await?;
    crate::sql::issue::edit(&mut tx, repo, number, title, body).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn add_dependency(
    pool: &SqlitePool,
    repo: i64,
    leased_issue: i64,
    blocker: i64,
    blocked: i64,
    lease: Option<&str>,
) -> Result<()> {
    if blocker == blocked {
        bail!("an issue cannot block itself");
    }
    require(pool, repo, blocker).await?;
    require(pool, repo, blocked).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, leased_issue, lease).await?;
    crate::sql::issue::insert_dependency(&mut tx, repo, blocker, blocked).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn remove_dependency(
    pool: &SqlitePool,
    repo: i64,
    leased_issue: i64,
    blocker: i64,
    blocked: i64,
    lease: Option<&str>,
) -> Result<()> {
    require(pool, repo, blocker).await?;
    require(pool, repo, blocked).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, leased_issue, lease).await?;
    crate::sql::issue::remove_dependency(&mut tx, repo, blocker, blocked).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn lock(pool: &SqlitePool, repo: i64, number: i64) -> Result<LeaseOutcome> {
    require(pool, repo, number).await?;
    match crate::sql::issue::acquire_lease(pool, repo, number).await? {
        Some(lease) => Ok(LeaseOutcome::Acquired(lease)),
        None => Ok(LeaseOutcome::AlreadyLeased),
    }
}

pub async fn unlock(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    lease: Option<&str>,
    force: bool,
) -> Result<bool> {
    require(pool, repo, number).await?;
    crate::sql::issue::release_lease(pool, repo, number, lease, force).await
}

pub async fn list_states(pool: &SqlitePool) -> Result<Vec<IssueState>> {
    crate::sql::issue::list_states(pool).await
}

pub async fn add_state(
    pool: &SqlitePool,
    name: &str,
    starting: bool,
    terminal: bool,
) -> Result<()> {
    if starting && terminal {
        bail!("a terminal state cannot be the state new issues start in");
    }
    if crate::sql::issue::get_state(pool, name).await?.is_some() {
        bail!("a configured state named {name:?} already exists");
    }
    crate::sql::issue::insert_state(pool, name, starting, terminal).await
}

async fn require_state(pool: &SqlitePool, name: &str) -> Result<IssueState> {
    crate::sql::issue::get_state(pool, name)
        .await?
        .ok_or_else(|| anyhow!("no configured state named {name:?}"))
}

/// Update a state's name or terminal flag.
///
/// Renaming repoints every issue in the state, so no issue is left pointing at
/// a name that no longer exists.
pub async fn set_state_config(
    pool: &SqlitePool,
    name: &str,
    new_name: Option<&str>,
    terminal: Option<bool>,
) -> Result<()> {
    let state = require_state(pool, name).await?;
    if new_name.is_none() && terminal.is_none() {
        bail!("nothing to update; pass --name or --terminal");
    }

    let terminal = terminal.unwrap_or(state.is_terminal);
    if state.is_starting && terminal {
        bail!("{name:?} is the starting state and cannot become terminal");
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
    if terminal != state.is_terminal {
        crate::sql::issue::update_state_flags(pool, &name, terminal).await?;
    }
    Ok(())
}

/// Delete a state, moving any issues that reference it to `move_to`.
pub async fn delete_state(pool: &SqlitePool, name: &str, move_to: Option<&str>) -> Result<i64> {
    let state = require_state(pool, name).await?;
    if state.is_starting {
        bail!("{name:?} is the starting state; run `octa config state set-default <name>` first");
    }
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

/// Make `name` the sole state new issues start in.
pub async fn set_default_state(pool: &SqlitePool, name: &str) -> Result<()> {
    let state = require_state(pool, name).await?;
    if state.is_terminal {
        bail!("terminal state {name:?} cannot be the starting state");
    }
    crate::sql::issue::set_starting_state(pool, name).await
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
        sqlx::query!("INSERT INTO repos (id, identity_key, name) VALUES (1, 'fixture', 'fixture')")
            .execute(&pool)
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
                "INSERT INTO issues (repo_id, number, title, state) VALUES (1, ?, ?, 'open')",
                number,
                title
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query!("INSERT INTO projects (repo_id, id, name) VALUES (1, 1, 'parent project')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query!(
            "INSERT INTO issue_projects (repo_id, issue_number, project_id) VALUES (1, 2, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        // Raw fixture deliberately models legacy/inconsistent data: #2 is
        // already below #1, but only #2 has a project.
        sqlx::query!(
            "INSERT INTO issue_parents (repo_id, child_number, parent_number) VALUES (1, 2, 1)",
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
            "SELECT COUNT(*) FROM issue_projects WHERE repo_id = 1 AND issue_number = 1"
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
            "INSERT INTO issues (repo_id, number, title, state) VALUES (1, 1, 'issue', 'open')",
        )
        .execute(&pool)
        .await
        .unwrap();
        for id in [1_i64, 2] {
            let name = format!("project {id}");
            sqlx::query!(
                "INSERT INTO projects (repo_id, id, name) VALUES (1, ?, ?)",
                id,
                name
            )
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query!(
            "INSERT INTO issue_projects (repo_id, issue_number, project_id) VALUES (1, 1, 1)",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO project_milestones \
             (repo_id, project_id, id, position, name) VALUES (1, 2, 1, 0, 'other')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let error = sqlx::query!(
            "INSERT INTO issue_milestones \
             (repo_id, issue_number, project_id, milestone_id) VALUES (1, 1, 2, 1)",
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
