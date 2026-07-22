//! Issue workflows: validation, policy, and composition of the SQL repository.

use crate::domain::issue::{Issue, IssueDetail, IssueState, LockOutcome, StateFilter};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;
use std::collections::{HashMap, HashSet};

async fn require(pool: &SqlitePool, repo: i64, number: i64) -> Result<Issue> {
    crate::sql::issue::get(pool, repo, number)
        .await?
        .ok_or_else(|| anyhow!("issue #{number} not found"))
}
async fn starting(pool: &SqlitePool, repo: i64) -> Result<String> {
    crate::sql::issue::default_starting_state(pool, repo)
        .await?
        .ok_or_else(|| anyhow!("no starting state configured for this repo"))
}
async fn terminal(pool: &SqlitePool, repo: i64) -> Result<String> {
    crate::sql::issue::default_terminal_state(pool, repo)
        .await?
        .ok_or_else(|| anyhow!("no terminal state configured for this repo"))
}

pub async fn create(pool: &SqlitePool, repo: i64, title: &str, body: &str) -> Result<i64> {
    crate::sql::issue::insert(pool, repo, title, body, &starting(pool, repo).await?).await
}

pub async fn list(
    pool: &SqlitePool,
    repo: Option<i64>,
    filter: StateFilter,
    state_name: Option<&str>,
    label: Option<&str>,
    unblocked: bool,
) -> Result<Vec<Issue>> {
    if repo.is_none() && (state_name.is_some() || label.is_some() || unblocked) {
        bail!("--state <name>, --label and --unblocked need a single repository");
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
    Ok(crate::sql::issue::list_entries(pool, repo)
        .await?
        .into_iter()
        .filter(|entry| {
            filter.includes(entry.is_terminal)
                && state_name.is_none_or(|state| entry.issue.state == state)
                && labelled
                    .as_ref()
                    .is_none_or(|set| set.contains(&entry.issue.number))
                && allowed
                    .as_ref()
                    .is_none_or(|set| set.contains(&entry.issue.number))
        })
        .map(|entry| entry.issue)
        .collect())
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
        comments: crate::sql::issue::comments(pool, repo, number).await?,
        issue,
    })
}
pub async fn comment(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    require(pool, repo, number).await?;
    crate::sql::issue::insert_comment(pool, repo, number, body).await?;
    crate::sql::issue::touch(pool, repo, number).await
}
pub async fn set_state(pool: &SqlitePool, repo: i64, number: i64, state: &str) -> Result<()> {
    require(pool, repo, number).await?;
    if !crate::sql::issue::state_exists(pool, repo, state).await? {
        bail!("unknown state {state:?}; add it first with `octa state add`");
    }
    crate::sql::issue::update_state(pool, repo, number, state).await
}
pub async fn close(pool: &SqlitePool, repo: i64, number: i64) -> Result<String> {
    let state = terminal(pool, repo).await?;
    set_state(pool, repo, number, &state).await?;
    Ok(state)
}
pub async fn reopen(pool: &SqlitePool, repo: i64, number: i64) -> Result<String> {
    let state = starting(pool, repo).await?;
    set_state(pool, repo, number, &state).await?;
    Ok(state)
}
pub async fn edit(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<()> {
    require(pool, repo, number).await?;
    if title.is_none() && body.is_none() {
        bail!("nothing to update: pass --title and/or --body");
    }
    if let Some(title) = title {
        crate::sql::issue::update_title(pool, repo, number, title).await?;
    }
    if let Some(body) = body {
        crate::sql::issue::update_body(pool, repo, number, body).await?;
    }
    crate::sql::issue::touch(pool, repo, number).await
}
pub async fn add_dependency(
    pool: &SqlitePool,
    repo: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    if blocker == blocked {
        bail!("an issue cannot block itself");
    }
    require(pool, repo, blocker).await?;
    require(pool, repo, blocked).await?;
    crate::sql::issue::insert_dependency(pool, repo, blocker, blocked).await
}
pub async fn remove_dependency(
    pool: &SqlitePool,
    repo: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    crate::sql::issue::remove_dependency(pool, repo, blocker, blocked).await
}
pub async fn lock(pool: &SqlitePool, repo: i64, number: i64, holder: &str) -> Result<LockOutcome> {
    require(pool, repo, number).await?;
    if crate::sql::issue::try_lock(pool, repo, number, holder).await? {
        Ok(LockOutcome::Acquired)
    } else {
        Ok(LockOutcome::AlreadyHeld(
            crate::sql::issue::locked_by(pool, repo, number)
                .await?
                .unwrap_or_else(|| "unknown".to_string()),
        ))
    }
}
pub async fn unlock(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    holder: &str,
    force: bool,
) -> Result<bool> {
    require(pool, repo, number).await?;
    crate::sql::issue::release_lock(pool, repo, number, (!force).then_some(holder)).await
}
pub async fn list_states(pool: &SqlitePool, repo: i64) -> Result<Vec<IssueState>> {
    crate::sql::issue::list_states(pool, repo).await
}
pub async fn add_state(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    starting: bool,
    terminal: bool,
) -> Result<()> {
    let position = crate::sql::issue::next_state_position(pool, repo).await?;
    crate::sql::issue::insert_state(pool, repo, name, starting, terminal, position).await
}
