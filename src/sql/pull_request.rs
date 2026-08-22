//! SQLite repository operations for pull requests. This module owns SQLx and
//! maps query results directly to domain values.

use crate::domain::{pull_request::PullRequest, Comment};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn insert(
    pool: &SqlitePool,
    repository: i64,
    title: &str,
    body: &str,
    branch: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"
            INSERT INTO pull_requests (repository_id, number, title, body, branch)
            VALUES (
                ?,
                (SELECT COALESCE(MAX(number), 0) + 1 FROM pull_requests WHERE repository_id = ?),
                ?, ?, ?
            )
            RETURNING number AS "number!: i64"
        "#,
        repository,
        repository,
        title,
        body,
        branch
    )
    .fetch_one(pool)
    .await?)
}

pub async fn insert_linked(
    pool: &SqlitePool,
    repository: i64,
    title: &str,
    body: &str,
    branch: &str,
    issue: i64,
    lease: Option<&str>,
) -> Result<i64> {
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repository, issue, lease).await?;
    let number = sqlx::query_scalar!(
        r#"INSERT INTO pull_requests (repository_id, number, title, body, branch)
           VALUES (
               ?,
               (SELECT COALESCE(MAX(number), 0) + 1 FROM pull_requests WHERE repository_id = ?),
               ?, ?, ?
           )
           RETURNING number AS "number!: i64""#,
        repository,
        repository,
        title,
        body,
        branch
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO issue_pull_request_links (repository_id, issue_number, pull_request_number) VALUES (?, ?, ?)",
        repository,
        issue,
        number
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(number)
}

pub async fn link_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    issue: i64,
    pull_request: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_pull_request_links (repository_id, issue_number, pull_request_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, issue_number, pull_request_number) DO NOTHING"#,
        repository,
        issue,
        pull_request
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn unlink_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    issue: i64,
    pull_request: i64,
) -> Result<bool> {
    Ok(sqlx::query!(
        "DELETE FROM issue_pull_request_links WHERE repository_id = ? AND issue_number = ? AND pull_request_number = ?",
        repository,
        issue,
        pull_request
    )
    .execute(&mut **tx)
    .await?
    .rows_affected()
        != 0)
}

pub async fn get(pool: &SqlitePool, repository: i64, number: i64) -> Result<Option<PullRequest>> {
    Ok(sqlx::query_as!(
        PullRequest,
        r#"
            SELECT
                r.name AS "repository!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                pull_requests p
            JOIN
                repositories r ON r.id = p.repository_id
            WHERE
                p.repository_id = ?
            AND
                p.number = ?
        "#,
        repository,
        number
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, repository: Option<i64>) -> Result<Vec<PullRequest>> {
    Ok(match repository {
        Some(repository) => {
            sqlx::query_as!(
                PullRequest,
                r#"
            SELECT
                r.name AS "repository!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                pull_requests p
            JOIN
                repositories r ON r.id = p.repository_id
            WHERE
                p.repository_id = ?
            ORDER BY
                p.number
        "#,
                repository
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                PullRequest,
                r#"
            SELECT
                r.name AS "repository!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                pull_requests p
            JOIN
                repositories r ON r.id = p.repository_id
            ORDER BY
                r.name, p.number
        "#
            )
            .fetch_all(pool)
            .await?
        }
    })
}

pub async fn comments(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<Comment>> {
    Ok(sqlx::query_as!(
        Comment,
        r#"
        SELECT
            id AS "id!: i64",
            body AS "body!: String",
            created_at AS "created_at!: String"
        FROM
            pull_request_comments
        WHERE
            repository_id = ?
        AND
            pull_request_number = ?
        ORDER BY
            id
    "#,
        repository,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn insert_comment(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    body: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO pull_request_comments (repository_id, pull_request_number, body) VALUES (?, ?, ?)",
        repository,
        number,
        body
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_state(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    state: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE pull_requests SET state = ?, updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        state,
        repository,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_title(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    title: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE pull_requests SET title = ? WHERE repository_id = ? AND number = ?",
        title,
        repository,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_body(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    body: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE pull_requests SET body = ? WHERE repository_id = ? AND number = ?",
        body,
        repository,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn touch(pool: &SqlitePool, repository: i64, number: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE pull_requests SET updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        repository,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}
