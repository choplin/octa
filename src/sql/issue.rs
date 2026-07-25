//! SQLite repository operations for Issue. This module owns SQLx and converts
//! database rows to domain values immediately; workflow policy lives in app.

use crate::domain::issue::{Issue, IssueListEntry, IssueRef, IssueState};
use crate::domain::milestone::MilestoneRef;
use crate::domain::Comment;
use crate::domain::{pr::PrRef, project::ProjectRef};
use anyhow::Result;
use sqlx::{Acquire, FromRow, SqlitePool};
use std::collections::HashSet;

#[derive(FromRow)]
struct IssueListRow {
    repo: String,
    number: i64,
    title: String,
    body: String,
    state: String,
    status_type: String,
    priority: i64,
    project_id: Option<i64>,
    project_name: Option<String>,
    milestone_id: Option<i64>,
    milestone_name: Option<String>,
    locked_by: Option<String>,
    created_at: String,
    updated_at: String,
    is_terminal: i64,
    state_position: i64,
}

pub async fn insert(
    pool: &SqlitePool,
    repo: i64,
    title: &str,
    body: &str,
    state: &str,
    priority: i64,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO issues (repo_id, number, title, body, state, priority)
           VALUES (?, (SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repo_id = ?), ?, ?, ?, ?)
           RETURNING number AS "number!: i64""#,
        repo,
        repo,
        title,
        body,
        state,
        priority
    )
    .fetch_one(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<Issue>> {
    let row = sqlx::query_as::<_, IssueListRow>(
        r#"
        SELECT
            r.name AS repo,
            i.number, i.title, i.body, i.state,
            COALESCE(s.status_type, 'unstarted') AS status_type,
            i.priority,
            p.id AS project_id, p.name AS project_name,
            m.id AS milestone_id, m.name AS milestone_name,
            i.locked_by, i.created_at, i.updated_at,
            COALESCE(s.is_terminal, 0) AS is_terminal,
            COALESCE(s.position, 0) AS state_position
        FROM
            issues i
        JOIN
            repos r ON r.id = i.repo_id
        LEFT JOIN
            issue_states s ON s.repo_id = i.repo_id
                AND s.name = i.state
        LEFT JOIN issue_projects ip
            ON ip.repo_id = i.repo_id AND ip.issue_number = i.number
        LEFT JOIN projects p
            ON p.repo_id = ip.repo_id AND p.id = ip.project_id
        LEFT JOIN issue_milestones im
            ON im.repo_id = i.repo_id AND im.issue_number = i.number
        LEFT JOIN project_milestones m
            ON m.repo_id = im.repo_id AND m.project_id = im.project_id
               AND m.id = im.milestone_id
        WHERE
            i.repo_id = ?
        AND
            i.number = ?
    "#,
    )
    .bind(repo)
    .bind(number)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| into_entry(row).issue))
}

pub async fn list_entries(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<IssueListEntry>> {
    let rows: Vec<IssueListRow> = match repo {
        Some(repo) => {
            sqlx::query_as::<_, IssueListRow>(
                r#"
            SELECT
                r.name AS repo,
                i.number, i.title, i.body, i.state,
                COALESCE(s.status_type, 'unstarted') AS status_type,
                i.priority,
                p.id AS project_id, p.name AS project_name,
                m.id AS milestone_id, m.name AS milestone_name,
                i.locked_by, i.created_at, i.updated_at,
                COALESCE(s.is_terminal, 0) AS is_terminal,
                COALESCE(s.position, 0) AS state_position
            FROM
                issues i
            JOIN
                repos r ON r.id = i.repo_id
            LEFT JOIN
                issue_states s ON s.repo_id = i.repo_id
                    AND s.name = i.state
            LEFT JOIN issue_projects ip
                ON ip.repo_id = i.repo_id AND ip.issue_number = i.number
            LEFT JOIN projects p
                ON p.repo_id = ip.repo_id AND p.id = ip.project_id
            LEFT JOIN issue_milestones im
                ON im.repo_id = i.repo_id AND im.issue_number = i.number
            LEFT JOIN project_milestones m
                ON m.repo_id = im.repo_id AND m.project_id = im.project_id
                   AND m.id = im.milestone_id
            WHERE
                i.repo_id = ?
            ORDER BY
                i.number
        "#,
            )
            .bind(repo)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as::<_, IssueListRow>(
                r#"
            SELECT
                r.name AS repo,
                i.number, i.title, i.body, i.state,
                COALESCE(s.status_type, 'unstarted') AS status_type,
                i.priority,
                p.id AS project_id, p.name AS project_name,
                m.id AS milestone_id, m.name AS milestone_name,
                i.locked_by, i.created_at, i.updated_at,
                COALESCE(s.is_terminal, 0) AS is_terminal,
                COALESCE(s.position, 0) AS state_position
            FROM
                issues i
            JOIN
                repos r ON r.id = i.repo_id
            LEFT JOIN
                issue_states s ON s.repo_id = i.repo_id
                    AND s.name = i.state
            LEFT JOIN issue_projects ip
                ON ip.repo_id = i.repo_id AND ip.issue_number = i.number
            LEFT JOIN projects p
                ON p.repo_id = ip.repo_id AND p.id = ip.project_id
            LEFT JOIN issue_milestones im
                ON im.repo_id = i.repo_id AND im.issue_number = i.number
            LEFT JOIN project_milestones m
                ON m.repo_id = im.repo_id AND m.project_id = im.project_id
                   AND m.id = im.milestone_id
            ORDER BY
                r.name, i.number
        "#,
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(into_entry).collect())
}

fn into_entry(row: IssueListRow) -> IssueListEntry {
    IssueListEntry {
        issue: Issue {
            repo: row.repo,
            number: row.number,
            title: row.title,
            body: row.body,
            state: row.state,
            status_type: row.status_type,
            priority: row.priority,
            project: row
                .project_id
                .zip(row.project_name)
                .map(|(id, name)| ProjectRef { id, name }),
            milestone: row
                .milestone_id
                .zip(row.milestone_name)
                .map(|(id, name)| MilestoneRef { id, name }),
            locked_by: row.locked_by,
            created_at: row.created_at,
            updated_at: row.updated_at,
        },
        is_terminal: row.is_terminal != 0,
        state_position: row.state_position,
    }
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

pub async fn related(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar::<_, i64>(
        r#"SELECT CASE
               WHEN low_number = ? THEN high_number
               ELSE low_number
           END
           FROM issue_relations
           WHERE repo_id = ? AND (low_number = ? OR high_number = ?)
           ORDER BY 1"#,
    )
    .bind(number)
    .bind(repo)
    .bind(number)
    .bind(number)
    .fetch_all(pool)
    .await?)
}

pub async fn linked_prs(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<PrRef>> {
    Ok(sqlx::query_as::<_, (i64, String, String, String)>(
        r#"SELECT p.number, p.title, p.branch, p.state
           FROM issue_pr_links l
           JOIN prs p ON p.repo_id = l.repo_id AND p.number = l.pr_number
           WHERE l.repo_id = ? AND l.issue_number = ?
           ORDER BY p.number"#,
    )
    .bind(repo)
    .bind(number)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|(number, title, branch, state)| PrRef {
        number,
        title,
        branch,
        state,
    })
    .collect())
}

pub async fn insert_relation(pool: &SqlitePool, repo: i64, a: i64, b: i64) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query(
        r#"INSERT INTO issue_relations (repo_id, low_number, high_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, low_number, high_number) DO NOTHING"#,
    )
    .bind(repo)
    .bind(low)
    .bind(high)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn remove_relation(pool: &SqlitePool, repo: i64, a: i64, b: i64) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query(
        "DELETE FROM issue_relations WHERE repo_id = ? AND low_number = ? AND high_number = ?",
    )
    .bind(repo)
    .bind(low)
    .bind(high)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn parent(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<IssueRef>> {
    Ok(sqlx::query_as!(
        IssueRef,
        r#"SELECT i.number AS "number!: i64", i.title AS "title!: String"
           FROM issue_parents p
           JOIN issues i ON i.repo_id = p.repo_id AND i.number = p.parent_number
           WHERE p.repo_id = ? AND p.child_number = ?"#,
        repo,
        number
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn children(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<IssueRef>> {
    Ok(sqlx::query_as!(
        IssueRef,
        r#"SELECT i.number AS "number!: i64", i.title AS "title!: String"
           FROM issue_parents p
           JOIN issues i ON i.repo_id = p.repo_id AND i.number = p.child_number
           WHERE p.repo_id = ? AND p.parent_number = ?
           ORDER BY i.number"#,
        repo,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn set_project(pool: &SqlitePool, repo: i64, number: i64, project_id: i64) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_projects (repo_id, issue_number, project_id)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
        repo,
        number,
        project_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn clear_project(pool: &SqlitePool, repo: i64, number: i64) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_projects WHERE repo_id = ? AND issue_number = ?",
        repo,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_parent(pool: &SqlitePool, repo: i64, child: i64, parent: i64) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_parents (repo_id, child_number, parent_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, child_number) DO UPDATE SET parent_number = excluded.parent_number"#,
        repo,
        child,
        parent
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically inherit a parent's project (when needed), install the parent
/// relation, and update the child's modification timestamp. A rejected cycle
/// or relation insert cannot leave a partially inherited project behind.
pub async fn set_parent_transactional(
    pool: &SqlitePool,
    repo: i64,
    child: i64,
    parent: i64,
    inherited_project: Option<i64>,
) -> Result<()> {
    let mut connection = pool.acquire().await?;
    let mut tx = connection.begin().await?;
    if let Some(project_id) = inherited_project {
        sqlx::query(
            r#"INSERT INTO issue_projects (repo_id, issue_number, project_id)
               VALUES (?, ?, ?)
               ON CONFLICT(repo_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
        )
        .bind(repo)
        .bind(child)
        .bind(project_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        r#"INSERT INTO issue_parents (repo_id, child_number, parent_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, child_number) DO UPDATE SET parent_number = excluded.parent_number"#,
    )
    .bind(repo)
    .bind(child)
    .bind(parent)
    .execute(&mut *tx)
    .await?;
    sqlx::query("UPDATE issues SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?")
        .bind(repo)
        .bind(child)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_parent(pool: &SqlitePool, repo: i64, child: i64) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_parents WHERE repo_id = ? AND child_number = ?",
        repo,
        child
    )
    .execute(pool)
    .await?;
    Ok(())
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

pub async fn update_priority(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    priority: i64,
) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET priority = ? WHERE repo_id = ? AND number = ?",
        priority,
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
    Ok(sqlx::query!(r#"SELECT name AS "name!: String", status_type AS "status_type!: String", is_starting AS "is_starting!: i64", is_terminal AS "is_terminal!: i64", position AS "position!: i64" FROM issue_states WHERE repo_id = ? ORDER BY position, name"#, repo).fetch_all(pool).await?.into_iter().map(|row| IssueState { name: row.name, status_type: row.status_type, is_starting: row.is_starting != 0, is_terminal: row.is_terminal != 0, position: row.position }).collect())
}

pub async fn insert_state(
    pool: &SqlitePool,
    repo: i64,
    name: &str,
    status_type: &str,
    starting: bool,
    terminal: bool,
    position: i64,
) -> Result<()> {
    let starting = starting as i64;
    let terminal = terminal as i64;
    sqlx::query!("INSERT INTO issue_states (repo_id, name, status_type, is_starting, is_terminal, position) VALUES (?, ?, ?, ?, ?, ?)", repo, name, status_type, starting, terminal, position).execute(pool).await?;
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
