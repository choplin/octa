//! Wiki primitive: repo-scoped pages that are octa entities (not repository
//! files), addressed by a slug and cross-linked via `[[slug]]` references.

use super::Store;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct WikiPage {
    pub repo: String,
    pub slug: String,
    pub title: String,
    pub body: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct WikiDetail {
    #[serde(flatten)]
    pub page: WikiPage,
    /// Slugs this page links to (from its body).
    pub links_to: Vec<String>,
    /// Slugs of pages that link to this page.
    pub backlinks: Vec<String>,
}

/// Extract `[[slug]]` targets from a page body, de-duplicated, order preserved.
fn parse_links(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(start) = rest.find("[[") {
        rest = &rest[start + 2..];
        if let Some(end) = rest.find("]]") {
            let target = super::slugify(&rest[..end]);
            if !target.is_empty() && !out.contains(&target) {
                out.push(target);
            }
            rest = &rest[end + 2..];
        } else {
            break;
        }
    }
    out
}

impl Store {
    pub async fn create_wiki(&self, slug: &str, title: &str, body: &str) -> Result<String> {
        let repo = self.repo_id()?;
        let slug = super::slugify(slug);
        if slug.is_empty() {
            bail!("slug is empty after normalization");
        }
        let exists = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM wiki_pages WHERE repo_id = ? AND slug = ?",
            repo,
            slug
        )
        .fetch_one(&self.pool)
        .await?;
        if exists != 0 {
            bail!("wiki page {slug:?} already exists; edit it instead");
        }
        sqlx::query!(
            "INSERT INTO wiki_pages (repo_id, slug, title, body) VALUES (?, ?, ?, ?)",
            repo,
            slug,
            title,
            body
        )
        .execute(&self.pool)
        .await?;
        self.sync_links(repo, &slug, body).await?;
        Ok(slug)
    }

    pub async fn edit_wiki(
        &self,
        slug: &str,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        let repo = self.repo_id()?;
        let slug = super::slugify(slug);
        self.require_wiki(&slug).await?;
        if title.is_none() && body.is_none() {
            bail!("nothing to update: pass --title and/or --body");
        }
        if let Some(t) = title {
            sqlx::query!(
                "UPDATE wiki_pages SET title = ?, updated_at = datetime('now') \
                 WHERE repo_id = ? AND slug = ?",
                t,
                repo,
                slug
            )
            .execute(&self.pool)
            .await?;
        }
        if let Some(b) = body {
            sqlx::query!(
                "UPDATE wiki_pages SET body = ?, updated_at = datetime('now') \
                 WHERE repo_id = ? AND slug = ?",
                b,
                repo,
                slug
            )
            .execute(&self.pool)
            .await?;
            self.sync_links(repo, &slug, b).await?;
        }
        Ok(())
    }

    async fn require_wiki(&self, slug: &str) -> Result<WikiPage> {
        match self.get_wiki(slug).await? {
            Some(p) => Ok(p),
            None => bail!("wiki page {slug:?} not found"),
        }
    }

    pub async fn get_wiki(&self, slug: &str) -> Result<Option<WikiPage>> {
        let repo = self.repo_id()?;
        let slug = super::slugify(slug);
        let page = sqlx::query_as!(
            WikiPage,
            r#"SELECT r.name       AS "repo!: String",
                      w.slug       AS "slug!: String",
                      w.title      AS "title!: String",
                      w.body       AS "body!: String",
                      w.created_at AS "created_at!: String",
                      w.updated_at AS "updated_at!: String"
               FROM wiki_pages w JOIN repos r ON r.id = w.repo_id
               WHERE w.repo_id = ? AND w.slug = ?"#,
            repo,
            slug
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(page)
    }

    pub async fn wiki_detail(&self, slug: &str) -> Result<WikiDetail> {
        let repo = self.repo_id()?;
        let slug = super::slugify(slug);
        let page = self.require_wiki(&slug).await?;
        let links_to = sqlx::query_scalar!(
            r#"SELECT to_slug AS "s!: String" FROM wiki_links
               WHERE repo_id = ? AND from_slug = ? ORDER BY to_slug"#,
            repo,
            slug
        )
        .fetch_all(&self.pool)
        .await?;
        let backlinks = sqlx::query_scalar!(
            r#"SELECT from_slug AS "s!: String" FROM wiki_links
               WHERE repo_id = ? AND to_slug = ? ORDER BY from_slug"#,
            repo,
            slug
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(WikiDetail {
            page,
            links_to,
            backlinks,
        })
    }

    pub async fn list_wiki(&self) -> Result<Vec<WikiPage>> {
        if self.is_all() {
            let pages = sqlx::query_as!(
                WikiPage,
                r#"SELECT r.name AS "repo!: String", w.slug AS "slug!: String",
                          w.title AS "title!: String", w.body AS "body!: String",
                          w.created_at AS "created_at!: String", w.updated_at AS "updated_at!: String"
                   FROM wiki_pages w JOIN repos r ON r.id = w.repo_id
                   ORDER BY r.name, w.slug"#
            )
            .fetch_all(&self.pool)
            .await?;
            return Ok(pages);
        }
        let repo = self.repo_id()?;
        let pages = sqlx::query_as!(
            WikiPage,
            r#"SELECT r.name AS "repo!: String", w.slug AS "slug!: String",
                      w.title AS "title!: String", w.body AS "body!: String",
                      w.created_at AS "created_at!: String", w.updated_at AS "updated_at!: String"
               FROM wiki_pages w JOIN repos r ON r.id = w.repo_id
               WHERE w.repo_id = ? ORDER BY w.slug"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(pages)
    }

    /// Re-derive the `[[slug]]` links for a page from its body.
    async fn sync_links(&self, repo: i64, from: &str, body: &str) -> Result<()> {
        sqlx::query!(
            "DELETE FROM wiki_links WHERE repo_id = ? AND from_slug = ?",
            repo,
            from
        )
        .execute(&self.pool)
        .await?;
        for target in parse_links(body) {
            if target == from {
                continue;
            }
            sqlx::query!(
                "INSERT OR IGNORE INTO wiki_links (repo_id, from_slug, to_slug) VALUES (?, ?, ?)",
                repo,
                from,
                target
            )
            .execute(&self.pool)
            .await?;
        }
        Ok(())
    }
}
