//! SQLite repository operations for Issue. This module owns SQLx and converts
//! database rows to domain values immediately; workflow policy lives in app.

use crate::domain::issue::{Issue, IssueListEntry, IssueState};
use crate::domain::Comment;
use anyhow::Result;
use sqlx::SqlitePool;
use std::collections::HashSet;

struct IssueListRow {
    repo: String,
    number: i64,
    title: String,
    body: String,
    state: String,
    locked_by: Option<String>,
    created_at: String,
    updated_at: String,
    is_terminal: i64,
}

pub async fn insert(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    state: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO issues (repo_id, number, title, body, state)
           VALUES (?, (SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repo_id = ?), ?, ?, ?)
           RETURNING number AS "number!: i64""#,
        repo,
        repo,
        title,
        body,
        state
    )
    .fetch_one(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<Issue>> {
    Ok(sqlx::query_as!(
        Issue,
        r#"
        SELECT
            r.name AS "repo!: String",
            i.number AS "number!: i64",
            i.title AS "title!: String",
            i.body AS "body!: String",
            i.state AS "state!: String",
            i.locked_by AS "locked_by?: String",
            i.created_at AS "created_at!: String",
            i.updated_at AS "updated_at!: String"
        FROM
            issues i
        JOIN
            repos r ON r.id = i.repo_id
        WHERE
            i.repo_id = ?
        AND
            i.number = ?
    "#,
        repo,
        number
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn list_entries(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<IssueListEntry>> {
    let rows: Vec<IssueListRow> = match repo {
        Some(repo) => {
            sqlx::query_as!(
                IssueListRow,
                r#"
            SELECT
                r.name AS "repo!: String",
                i.number AS "number!: i64",
                i.title AS "title!: String",
                i.body AS "body!: String",
                i.state AS "state!: String",
                i.locked_by AS "locked_by?: String",
                i.created_at AS "created_at!: String",
                i.updated_at AS "updated_at!: String",
                COALESCE(s.is_terminal, 0) AS "is_terminal!: i64"
            FROM
                issues i
            JOIN
                repos r ON r.id = i.repo_id
            LEFT JOIN
                issue_states s ON s.repo_id = i.repo_id
                    AND s.name = i.state
            WHERE
                i.repo_id = ?
            ORDER BY
                i.number
        "#,
                repo
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                IssueListRow,
                r#"
            SELECT
                r.name AS "repo!: String",
                i.number AS "number!: i64",
                i.title AS "title!: String",
                i.body AS "body!: String",
                i.state AS "state!: String",
                i.locked_by AS "locked_by?: String",
                i.created_at AS "created_at!: String",
                i.updated_at AS "updated_at!: String",
                COALESCE(s.is_terminal, 0) AS "is_terminal!: i64"
            FROM
                issues i
            JOIN
                repos r ON r.id = i.repo_id
            LEFT JOIN
                issue_states s ON s.repo_id = i.repo_id
                    AND s.name = i.state
            ORDER BY
                r.name, i.number
        "#
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|row| IssueListEntry {
            issue: Issue {
                repo: row.repo,
                number: row.number,
                title: row.title,
                body: row.body,
                state: row.state,
                locked_by: row.locked_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
            },
            is_terminal: row.is_terminal != 0,
        })
        .collect())
}

pub async fn labelled_numbers(pool: &SqlitePool, repo: i64, label: &str) -> Result<HashSet<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT issue_number AS "n!: i64" FROM issue_labels WHERE repo_id = ? AND label_name = ?"#, repo, label).fetch_all(pool).await?.into_iter().collect())
}
pub async fn state_flags(pool: &SqlitePool, repo: i64) -> Result<Vec<(i64, bool)>> {
    Ok(sqlx::query!(r#"SELECT i.number AS "number!: i64", COALESCE(s.is_terminal, 0) AS "is_terminal!: i64" FROM issues i LEFT JOIN issue_states s ON s.repo_id = i.repo_id AND s.name = i.state WHERE i.repo_id = ?"#, repo).fetch_all(pool).await?.into_iter().map(|row| (row.number, row.is_terminal != 0)).collect())
}
pub async fn dependencies(pool: &SqlitePool, repo: i64) -> Result<Vec<(i64, i64)>> {
    Ok(sqlx::query!(r#"SELECT blocker_number AS "blocker!: i64", blocked_number AS "blocked!: i64" FROM issue_deps WHERE repo_id = ?"#, repo).fetch_all(pool).await?.into_iter().map(|row| (row.blocker, row.blocked)).collect())
}

pub async fn comments(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<Comment>> {
    Ok(sqlx::query_as!(Comment, r#"SELECT id AS "id!: i64", body AS "body!: String", created_at AS "created_at!: String" FROM comments WHERE repo_id = ? AND issue_number = ? ORDER BY id"#, repo, number).fetch_all(pool).await?)
}
pub async fn labels(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(r#"SELECT label_name AS "l!: String" FROM issue_labels WHERE repo_id = ? AND issue_number = ? ORDER BY label_name"#, repo, number).fetch_all(pool).await?)
}
pub async fn blocks(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT blocked_number AS "n!: i64" FROM issue_deps WHERE repo_id = ? AND blocker_number = ? ORDER BY blocked_number"#, repo, number).fetch_all(pool).await?)
}
pub async fn blocked_by(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT blocker_number AS "n!: i64" FROM issue_deps WHERE repo_id = ? AND blocked_number = ? ORDER BY blocker_number"#, repo, number).fetch_all(pool).await?)
}

pub async fn insert_comment(pool: &SqlitePool, repo: i64, number: i64, body: &str) -> Result<()> {
    sqlx::query!(
        "INSERT INTO comments (repo_id, issue_number, body) VALUES (?, ?, ?)",
        repo,
        number,
        body
    )
    .execute(pool)
    .await?;
    Ok(())
}
pub async fn state_exists(pool: &SqlitePool, repo: i64, state: &str) -> Result<bool> {
    Ok(sqlx::query_scalar!(
        "SELECT COUNT(*) FROM issue_states WHERE repo_id = ? AND name = ?",
        repo,
        state
    )
    .fetch_one(pool)
    .await?
        != 0)
}
pub async fn update_state(pool: &SqlitePool, repo: i64, number: i64, state: &str) -> Result<()> {
    sqlx::query!("UPDATE issues SET state = ?, updated_at = datetime('now') WHERE repo_id = ? AND number = ?", state, repo, number).execute(pool).await?;
    Ok(())
}
pub async fn update_title(pool: &SqlitePool, repo: i64, number: i64, title: &str) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET title = ? WHERE repo_id = ? AND number = ?",
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
        "UPDATE issues SET body = ? WHERE repo_id = ? AND number = ?",
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
        "UPDATE issues SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}
pub async fn insert_dependency(
    pool: &SqlitePool,
    repo: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    sqlx::query!("INSERT OR IGNORE INTO issue_deps (repo_id, blocker_number, blocked_number) VALUES (?, ?, ?)", repo, blocker, blocked).execute(pool).await?;
    Ok(())
}
pub async fn remove_dependency(
    pool: &SqlitePool,
    repo: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_deps WHERE repo_id = ? AND blocker_number = ? AND blocked_number = ?",
        repo,
        blocker,
        blocked
    )
    .execute(pool)
    .await?;
    Ok(())
}
pub async fn try_lock(pool: &SqlitePool, repo: i64, number: i64, holder: &str) -> Result<bool> {
    Ok(sqlx::query!("UPDATE issues SET locked_by = ?, locked_at = datetime('now') WHERE repo_id = ? AND number = ? AND locked_by IS NULL", holder, repo, number).execute(pool).await?.rows_affected() == 1)
}
pub async fn locked_by(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<String>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT locked_by AS "locked_by?: String" FROM issues WHERE repo_id = ? AND number = ?"#,
        repo,
        number
    )
    .fetch_one(pool)
    .await?)
}
pub async fn release_lock(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    holder: Option<&str>,
) -> Result<bool> {
    let result = match holder { Some(holder) => sqlx::query!("UPDATE issues SET locked_by = NULL, locked_at = NULL WHERE repo_id = ? AND number = ? AND locked_by = ?", repo, number, holder).execute(pool).await?, None => sqlx::query!("UPDATE issues SET locked_by = NULL, locked_at = NULL WHERE repo_id = ? AND number = ?", repo, number).execute(pool).await?, };
    Ok(result.rows_affected() == 1)
}
pub async fn list_states(pool: &SqlitePool, repo: i64) -> Result<Vec<IssueState>> {
    Ok(sqlx::query!(r#"SELECT name AS "name!: String", is_starting AS "is_starting!: i64", is_terminal AS "is_terminal!: i64", position AS "position!: i64" FROM issue_states WHERE repo_id = ? ORDER BY position, name"#, repo).fetch_all(pool).await?.into_iter().map(|row| IssueState { name: row.name, is_starting: row.is_starting != 0, is_terminal: row.is_terminal != 0, position: row.position }).collect())
}
pub async fn insert_state(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    starting: bool,
    terminal: bool,
    position: i64,
) -> Result<()> {
    let starting = starting as i64;
    let terminal = terminal as i64;
    sqlx::query!("INSERT INTO issue_states (repo_id, name, is_starting, is_terminal, position) VALUES (?, ?, ?, ?, ?)", repo, name, starting, terminal, position).execute(pool).await?;
    Ok(())
}
pub async fn next_state_position(pool: &SqlitePool, repo: i64) -> Result<i64> {
    Ok(sqlx::query_scalar!(r#"SELECT COALESCE(MAX(position), -1) + 1 AS "p!: i64" FROM issue_states WHERE repo_id = ?"#, repo).fetch_one(pool).await?)
}
pub async fn default_starting_state(pool: &SqlitePool, repo: i64) -> Result<Option<String>> {
    Ok(sqlx::query_scalar!("SELECT name FROM issue_states WHERE repo_id = ? AND is_starting = 1 ORDER BY position LIMIT 1", repo).fetch_optional(pool).await?)
}
pub async fn default_terminal_state(pool: &SqlitePool, repo: i64) -> Result<Option<String>> {
    Ok(sqlx::query_scalar!("SELECT name FROM issue_states WHERE repo_id = ? AND is_terminal = 1 ORDER BY position LIMIT 1", repo).fetch_optional(pool).await?)
}
