//! SQLite repository operations for pull requests. This module owns SQLx and
//! maps query results directly to domain values.

use crate::domain::{pr::Pr, Comment};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};

pub async fn insert(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    branch: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"
            INSERT INTO prs (repo_id, number, title, body, branch)
            VALUES (
                ?,
                (SELECT COALESCE(MAX(number), 0) + 1 FROM prs WHERE repo_id = ?),
                ?, ?, ?
            )
            RETURNING number AS "number!: i64"
        "#,
        repo,
        repo,
        title,
        body,
        branch
    )
    .fetch_one(pool)
    .await?)
}

pub async fn insert_linked(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    branch: &str,
    issue: i64,
    lease: Option<&str>,
) -> Result<i64> {
    let mut tx = crate::sql::issue::begin_lease_mutation(pool, repo, issue, lease).await?;
    let number = sqlx::query_scalar!(
        r#"INSERT INTO prs (repo_id, number, title, body, branch)
           VALUES (
               ?,
               (SELECT COALESCE(MAX(number), 0) + 1 FROM prs WHERE repo_id = ?),
               ?, ?, ?
           )
           RETURNING number AS "number!: i64""#,
        repo,
        repo,
        title,
        body,
        branch
    )
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query!(
        "INSERT INTO issue_pr_links (repo_id, issue_number, pr_number) VALUES (?, ?, ?)",
        repo,
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
    repo: i64,
    issue: i64,
    pr: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_pr_links (repo_id, issue_number, pr_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, issue_number, pr_number) DO NOTHING"#,
        repo,
        issue,
        pr
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn unlink_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    issue: i64,
    pr: i64,
) -> Result<bool> {
    Ok(sqlx::query!(
        "DELETE FROM issue_pr_links WHERE repo_id = ? AND issue_number = ? AND pr_number = ?",
        repo,
        issue,
        pr
    )
    .execute(&mut **tx)
    .await?
    .rows_affected()
        != 0)
}

pub async fn get(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<Pr>> {
    Ok(sqlx::query_as!(
        Pr,
        r#"
            SELECT
                r.name AS "repo!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                prs p
            JOIN
                repos r ON r.id = p.repo_id
            WHERE
                p.repo_id = ?
            AND
                p.number = ?
        "#,
        repo,
        number
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn list(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<Pr>> {
    Ok(match repo {
        Some(repo) => {
            sqlx::query_as!(
                Pr,
                r#"
            SELECT
                r.name AS "repo!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                prs p
            JOIN
                repos r ON r.id = p.repo_id
            WHERE
                p.repo_id = ?
            ORDER BY
                p.number
        "#,
                repo
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                Pr,
                r#"
            SELECT
                r.name AS "repo!: String",
                p.number AS "number!: i64",
                p.title AS "title!: String",
                p.body AS "body!: String",
                p.branch AS "branch!: String",
                p.state AS "state!: String",
                p.created_at AS "created_at!: String",
                p.updated_at AS "updated_at!: String"
            FROM
                prs p
            JOIN
                repos r ON r.id = p.repo_id
            ORDER BY
                r.name, p.number
        "#
            )
            .fetch_all(pool)
            .await?
        }
    })
}

pub async fn comments(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<Comment>> {
    Ok(sqlx::query_as!(
        Comment,
        r#"
        SELECT
            id AS "id!: i64",
            body AS "body!: String",
            created_at AS "created_at!: String"
        FROM
            pr_comments
        WHERE
            repo_id = ?
        AND
            pr_number = ?
        ORDER BY
            id
    "#,
        repo,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn insert_comment(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO pr_comments (repo_id, pr_number, body) VALUES (?, ?, ?)",
        repo,
        number,
        body
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_state(pool: &SqlitePool, repo: i64, number: i64, state: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE prs SET state = ?, updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        state,
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_title(pool: &SqlitePool, repo: i64, number: i64, title: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE prs SET title = ? WHERE repo_id = ? AND number = ?",
        title,
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_body(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE prs SET body = ? WHERE repo_id = ? AND number = ?",
        body,
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn touch(pool: &SqlitePool, repo: i64, number: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE prs SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}
