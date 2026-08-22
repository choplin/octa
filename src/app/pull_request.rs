//! Pull-request workflows: validation and composition of the SQL repository.

use crate::domain::{
    pull_request::{PullRequest, PullRequestDetail},
    StateFilter,
};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;

async fn require(pool: &SqlitePool, repository: i64, number: i64) -> Result<PullRequest> {
    crate::sql::pull_request::get(pool, repository, number)
        .await?
        .ok_or_else(|| anyhow!("pull request #{number} not found"))
}

pub async fn create(
    pool: &SqlitePool,
    repository: i64,
    title: &str,
    body: &str,
    branch: &str,
    issue: Option<i64>,
    lease: Option<&str>,
) -> Result<i64> {
    match issue {
        Some(issue) => {
            crate::sql::issue::get(pool, repository, issue)
                .await?
                .ok_or_else(|| anyhow!("issue #{issue} not found"))?;
            crate::sql::pull_request::insert_linked(
                pool, repository, title, body, branch, issue, lease,
            )
            .await
        }
        None => crate::sql::pull_request::insert(pool, repository, title, body, branch).await,
    }
}

pub async fn link(
    pool: &SqlitePool,
    repository: i64,
    issue: i64,
    pull_request: i64,
    lease: Option<&str>,
) -> Result<()> {
    crate::sql::issue::get(pool, repository, issue)
        .await?
        .ok_or_else(|| anyhow!("issue #{issue} not found"))?;
    require(pool, repository, pull_request).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, issue, lease).await?;
    crate::sql::pull_request::link_tx(&mut tx, repository, issue, pull_request).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn unlink(
    pool: &SqlitePool,
    repository: i64,
    issue: i64,
    pull_request: i64,
    lease: Option<&str>,
) -> Result<()> {
    crate::sql::issue::get(pool, repository, issue)
        .await?
        .ok_or_else(|| anyhow!("issue #{issue} not found"))?;
    require(pool, repository, pull_request).await?;
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, issue, lease).await?;
    if !crate::sql::pull_request::unlink_tx(&mut tx, repository, issue, pull_request).await? {
        bail!("issue #{issue} is not linked to pull request #{pull_request}");
    }
    tx.commit().await?;
    Ok(())
}

pub async fn list(
    pool: &SqlitePool,
    repository: Option<i64>,
    filter: StateFilter,
) -> Result<Vec<PullRequest>> {
    Ok(crate::sql::pull_request::list(pool, repository)
        .await?
        .into_iter()
        .filter(|pull_request| match filter {
            StateFilter::Open => pull_request.state == "open",
            StateFilter::Closed => pull_request.state != "open",
            StateFilter::All => true,
        })
        .collect())
}

pub async fn detail(pool: &SqlitePool, repository: i64, number: i64) -> Result<PullRequestDetail> {
    Ok(PullRequestDetail {
        pull_request: require(pool, repository, number).await?,
        comments: crate::sql::pull_request::comments(pool, repository, number).await?,
    })
}

pub async fn comment(pool: &SqlitePool, repository: i64, number: i64, body: &str) -> Result<()> {
    require(pool, repository, number).await?;
    crate::sql::pull_request::insert_comment(pool, repository, number, body).await?;
    crate::sql::pull_request::touch(pool, repository, number).await
}

pub async fn set_state(pool: &SqlitePool, repository: i64, number: i64, state: &str) -> Result<()> {
    require(pool, repository, number).await?;
    crate::sql::pull_request::update_state(pool, repository, number, state).await
}

pub async fn edit(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<()> {
    require(pool, repository, number).await?;
    if title.is_none() && body.is_none() {
        bail!("nothing to update: pass --title and/or --body");
    }
    if let Some(title) = title {
        crate::sql::pull_request::update_title(pool, repository, number, title).await?;
    }
    if let Some(body) = body {
        crate::sql::pull_request::update_body(pool, repository, number, body).await?;
    }
    crate::sql::pull_request::touch(pool, repository, number).await
}
