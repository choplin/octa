use crate::domain::wiki::WikiPage;
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn exists(pool: &SqlitePool, repository: i64, slug: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM wiki_pages WHERE repository_id = ? AND slug = ?",
        repository,
        slug
    )
    .fetch_one(pool)
    .await?
        != 0)
}

pub async fn get(pool: &SqlitePool, repository: i64, slug: &str) -> Result<Option<WikiPage>> {
    Ok(sqlx::query_as!(
        WikiPage,
        r#"
        SELECT
            r.name AS "repository!: String",
            w.slug AS "slug!: String",
            w.title AS "title!: String",
            w.body AS "body!: String",
            w.created_at AS "created_at!: String",
            w.updated_at AS "updated_at!: String"
        FROM
            wiki_pages w
        JOIN
            repositories r ON r.id = w.repository_id
        WHERE
            w.repository_id = ?
        AND
            w.slug = ?
    "#,
        repository,
        slug
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, repository: Option<i64>) -> Result<Vec<WikiPage>> {
    Ok(match repository {
        Some(repository) => {
            sqlx::query_as!(
                WikiPage,
                r#"
            SELECT
                r.name AS "repository!: String",
                w.slug AS "slug!: String",
                w.title AS "title!: String",
                w.body AS "body!: String",
                w.created_at AS "created_at!: String",
                w.updated_at AS "updated_at!: String"
            FROM
                wiki_pages w
            JOIN
                repositories r ON r.id = w.repository_id
            WHERE
                w.repository_id = ?
            ORDER BY
                w.slug
        "#,
                repository
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                WikiPage,
                r#"
            SELECT
                r.name AS "repository!: String",
                w.slug AS "slug!: String",
                w.title AS "title!: String",
                w.body AS "body!: String",
                w.created_at AS "created_at!: String",
                w.updated_at AS "updated_at!: String"
            FROM
                wiki_pages w
            JOIN
                repositories r ON r.id = w.repository_id
            ORDER BY
                r.name, w.slug
        "#
            )
            .fetch_all(pool)
            .await?
        }
    })
}

pub async fn links_to(pool: &SqlitePool, repository: i64, slug: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT to_slug AS "s!: String"
        FROM
            wiki_links
        WHERE
            repository_id = ?
        AND
            from_slug = ?
        ORDER BY
            to_slug
    "#,
        repository,
        slug
    )
    .fetch_all(pool)
    .await?)
}

pub async fn backlinks(pool: &SqlitePool, repository: i64, slug: &str) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(
        r#"
        SELECT from_slug AS "s!: String"
        FROM
            wiki_links
        WHERE
            repository_id = ?
        AND
            to_slug = ?
        ORDER BY
            from_slug
    "#,
        repository,
        slug
    )
    .fetch_all(pool)
    .await?)
}

pub async fn insert(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    slug: &str,
    title: &str,
    body: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO wiki_pages (repository_id, slug, title, body) VALUES (?, ?, ?, ?)",
        repository,
        slug,
        title,
        body
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn update_title(
    pool: &SqlitePool,
    repository: i64,
    slug: &str,
    title: &str,
) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET title = ?, updated_at = datetime('now') WHERE repository_id = ? AND slug = ?", title, repository, slug).execute(pool).await?;
    Ok(())
}

pub async fn update_title_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    slug: &str,
    title: &str,
) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET title = ?, updated_at = datetime('now') WHERE repository_id = ? AND slug = ?", title, repository, slug).execute(&mut **tx).await?;
    Ok(())
}

pub async fn update_body(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    slug: &str,
    body: &str,
) -> Result<()> {
    sqlx::query!("UPDATE wiki_pages SET body = ?, updated_at = datetime('now') WHERE repository_id = ? AND slug = ?", body, repository, slug).execute(&mut **tx).await?;
    Ok(())
}

pub async fn sync_links(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    from: &str,
    links: &[String],
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM wiki_links WHERE repository_id = ? AND from_slug = ?",
        repository,
        from
    )
    .execute(&mut **tx)
    .await?;
    for target in links {
        if target != from {
            sqlx::query!(
                "INSERT OR IGNORE INTO wiki_links (repository_id, from_slug, to_slug) VALUES (?, ?, ?)",
                repository,
                from,
                target
            )
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}
