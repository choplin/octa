use crate::domain::wiki::{parse_links, WikiDetail, WikiPage};
use anyhow::{anyhow, bail, Result};
use sqlx::SqlitePool;
fn slug(value: &str) -> Result<String> {
    let value = crate::domain::wiki::slugify(value);
    if value.is_empty() {
        bail!("slug is empty after normalization");
    }
    Ok(value)
}
async fn require(pool: &SqlitePool, repo: i64, slug: &str) -> Result<WikiPage> {
    crate::sql::wiki::get(pool, repo, slug)
        .await?
        .ok_or_else(|| anyhow!("wiki page {slug:?} not found"))
}

pub async fn create(
    pool: &SqlitePool,
    repo: i64,
    raw_slug: &str,
    title: &str,
    body: &str,
) -> Result<String> {
    let slug = slug(raw_slug)?;
    if crate::sql::wiki::exists(pool, repo, &slug).await? {
        bail!("wiki page {slug:?} already exists; edit it instead");
    }
    let mut tx = pool.begin().await?;
    crate::sql::wiki::insert(&mut tx, repo, &slug, title, body).await?;
    crate::sql::wiki::sync_links(&mut tx, repo, &slug, &parse_links(body)).await?;
    tx.commit().await?;
    Ok(slug)
}

pub async fn edit(
    pool: &SqlitePool,
    repo: i64,
    raw_slug: &str,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<()> {
    let slug = slug(raw_slug)?;
    require(pool, repo, &slug).await?;
    if title.is_none() && body.is_none() {
        bail!("nothing to update: pass --title and/or --body");
    }
    if let Some(body) = body {
        let mut tx = pool.begin().await?;
        if let Some(title) = title {
            crate::sql::wiki::update_title_tx(&mut tx, repo, &slug, title).await?;
        }
        crate::sql::wiki::update_body(&mut tx, repo, &slug, body).await?;
        crate::sql::wiki::sync_links(&mut tx, repo, &slug, &parse_links(body)).await?;
        tx.commit().await?;
    } else if let Some(title) = title {
        crate::sql::wiki::update_title(pool, repo, &slug, title).await?;
    }
    Ok(())
}

pub async fn detail(pool: &SqlitePool, repo: i64, raw_slug: &str) -> Result<WikiDetail> {
    let slug = slug(raw_slug)?;
    Ok(WikiDetail {
        page: require(pool, repo, &slug).await?,
        links_to: crate::sql::wiki::links_to(pool, repo, &slug).await?,
        backlinks: crate::sql::wiki::backlinks(pool, repo, &slug).await?,
    })
}

pub async fn list(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<WikiPage>> {
    crate::sql::wiki::list(pool, repo).await
}
