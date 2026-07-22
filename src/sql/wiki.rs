use crate::domain::wiki::WikiPage;
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn exists(pool: &SqlitePool, repo: i64, slug: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM wiki_pages WHERE repo_id = ? AND slug = ?",
        repo,
        slug
    )
    .fetch_one(pool)
    .await?
        != 0)
}
pub async fn get(pool: &SqlitePool, repo: i64, slug: &str) -> Result<Option<WikiPage>> {
    Ok(sqlx::query_as!(
        WikiPage,
        r#"
        SELECT
            r.name AS "repo!: String",
            w.slug AS "slug!: String",
            w.title AS "title!: String",
            w.body AS "body!: String",
            w.created_at AS "created_at!: String",
            w.updated_at AS "updated_at!: String"
        FROM wiki_pages w
        JOIN repos r ON r.id = w.repo_id
        WHERE w.repo_id = ?
          AND w.slug = ?
    "#,
        repo,
        slug
    )
    .fetch_optional(pool)
    .await?)
}
pub async fn list(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<WikiPage>> {
    Ok(match repo { Some(repo) => sqlx::query_as!(WikiPage, r#"SELECT r.name AS "repo!: String", w.slug AS "slug!: String", w.title AS "title!: String", w.body AS "body!: String", w.created_at AS "created_at!: String", w.updated_at AS "updated_at!: String" FROM wiki_pages w JOIN repos r ON r.id = w.repo_id WHERE w.repo_id = ? ORDER BY w.slug"#, repo).fetch_all(pool).await?, None => sqlx::query_as!(WikiPage, r#"SELECT r.name AS "repo!: String", w.slug AS "slug!: String", w.title AS "title!: String", w.body AS "body!: String", w.created_at AS "created_at!: String", w.updated_at AS "updated_at!: String" FROM wiki_pages w JOIN repos r ON r.id = w.repo_id ORDER BY r.name, w.slug"#).fetch_all(pool).await? })
}
pub async fn links_to(pool: &SqlitePool, repo: i64, slug: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT to_slug AS "s!: String"
        FROM wiki_links
        WHERE repo_id = ?
          AND from_slug = ?
        ORDER BY to_slug
    "#,
        repo,
        slug
    )
    .fetch_all(pool)
    .await?)
}
pub async fn backlinks(pool: &SqlitePool, repo: i64, slug: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT from_slug AS "s!: String"
        FROM wiki_links
        WHERE repo_id = ?
          AND to_slug = ?
        ORDER BY from_slug
    "#,
        repo,
        slug
    )
    .fetch_all(pool)
    .await?)
}
pub async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    slug: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO wiki_pages (repo_id, slug, title, body) VALUES (?, ?, ?, ?)",
        repo,
        slug,
        title,
        body
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
pub async fn update_title(pool: &SqlitePool, repo: i64, slug: &str, title: &str) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET title = ?, updated_at = datetime('now') WHERE repo_id = ? AND slug = ?", title, repo, slug).execute(pool).await?;
    Ok(())
}
pub async fn update_title_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    slug: &str,
    title: &str,
) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET title = ?, updated_at = datetime('now') WHERE repo_id = ? AND slug = ?", title, repo, slug).execute(&mut **tx).await?;
    Ok(())
}
pub async fn update_body(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    slug: &str,
    body: &str,
) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET body = ?, updated_at = datetime('now') WHERE repo_id = ? AND slug = ?", body, repo, slug).execute(&mut **tx).await?;
    Ok(())
}
pub async fn sync_links(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    from: &str,
    links: &[String],
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM wiki_links WHERE repo_id = ? AND from_slug = ?",
        repo,
        from
    )
    .execute(&mut **tx)
    .await?;
    for target in links {
        if target != from {
            sqlx::query!(
                "INSERT OR IGNORE INTO wiki_links (repo_id, from_slug, to_slug) VALUES (?, ?, ?)",
                repo,
                from,
                target
            )
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
