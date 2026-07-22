//! Pull-request workflows: validation and composition of the SQL repository.

use crate::domain::{
    pr::{Pr, PrDetail},
    StateFilter,
};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;

async fn require(pool: &SqlitePool, repo: i64, number: i64) -> Result<Pr> {
    crate::sql::pr::get(pool, repo, number)
        .await?
        .ok_or_else(|| anyhow!("PR #{number} not found"))
}

pub async fn create(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    branch: &str,
) -> Result<i64> {
    crate::sql::pr::insert(pool, repo, title, body, branch).await
}

pub async fn list(pool: &SqlitePool, repo: Option<i64>, filter: StateFilter) -> Result<Vec<Pr>> {
    Ok(crate::sql::pr::list(pool, repo)
        .await?
        .into_iter()
        .filter(|pr| match filter {
            StateFilter::Open => pr.state == "open",
            StateFilter::Closed => pr.state != "open",
            StateFilter::All => true,
        })
        .collect())
}

pub async fn detail(pool: &SqlitePool, repo: i64, number: i64) -> Result<PrDetail> {
    Ok(PrDetail {
        pr: require(pool, repo, number).await?,
        comments: crate::sql::pr::comments(pool, repo, number).await?,
    })
}

pub async fn comment(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    require(pool, repo, number).await?;
    crate::sql::pr::insert_comment(pool, repo, number, body).await?;
    crate::sql::pr::touch(pool, repo, number).await
}

pub async fn set_state(pool: &SqlitePool, repo: i64, number: i64, state: &str) -> Result<()> {
    require(pool, repo, number).await?;
    crate::sql::pr::update_state(pool, repo, number, state).await
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
        crate::sql::pr::update_title(pool, repo, number, title).await?;
    }
    if let Some(body) = body {
        crate::sql::pr::update_body(pool, repo, number, body).await?;
    }
    crate::sql::pr::touch(pool, repo, number).await
}
