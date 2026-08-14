//! SQLite repository operations for Issue. This module owns SQLx and converts
//! database rows to domain values immediately; workflow policy lives in app.

use crate::domain::issue::{Issue, IssueListEntry, IssueRef, IssueState};
use crate::domain::milestone::MilestoneRef;
use crate::domain::Comment;
use crate::domain::{pr::PrRef, project::ProjectRef};
use anyhow::Result;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::collections::HashSet;

const LEASE_ADJECTIVES: &[&str] = &[
    "amber", "azure", "bold", "brisk", "calm", "cedar", "clear", "coral", "cosmic", "crisp",
    "dawn", "eager", "ember", "fair", "fern", "gentle", "golden", "grand", "green", "happy",
    "hollow", "indigo", "ivory", "jade", "keen", "lively", "lucid", "lunar", "maple", "mellow",
    "merry", "misty", "navy", "nimble", "noble", "olive", "pearl", "pine", "quiet", "rapid", "red",
    "river", "royal", "ruby", "sage", "silver", "solar", "steady", "still", "sunny", "swift",
    "teal", "tidy", "umber", "vivid", "warm", "white", "wild", "wise", "witty", "yellow", "young",
    "zesty", "bright",
];
const LEASE_ANIMALS: &[&str] = &[
    "badger", "bear", "beaver", "bison", "bobcat", "camel", "caribou", "cat", "cougar", "crane",
    "crow", "deer", "dingo", "dolphin", "eagle", "falcon", "ferret", "finch", "fox", "gecko",
    "goat", "goose", "heron", "horse", "ibis", "jaguar", "koala", "lemur", "lion", "lynx",
    "marten", "moose", "mouse", "newt", "otter", "owl", "panda", "parrot", "puma", "quail",
    "rabbit", "raven", "robin", "seal", "shark", "sheep", "skunk", "sloth", "sparrow", "swan",
    "tiger", "toad", "turtle", "vole", "walrus", "weasel", "whale", "wolf", "wombat", "yak",
    "zebra", "antelope", "donkey", "gopher",
];
const LEASE_OBJECTS: &[&str] = &[
    "anchor", "arch", "bell", "bridge", "brook", "cabin", "canyon", "castle", "cave", "cliff",
    "cloud", "comet", "compass", "cove", "creek", "crystal", "dune", "field", "forest", "garden",
    "gate", "glade", "grove", "harbor", "haven", "hill", "island", "key", "lake", "lantern",
    "meadow", "mesa", "moon", "mountain", "oasis", "ocean", "orbit", "path", "peak", "pond",
    "rain", "reef", "ridge", "road", "rock", "shore", "sky", "spring", "star", "stone", "stream",
    "sun", "tower", "trail", "tree", "valley", "wave", "willow", "wind", "wood", "harvest",
    "horizon", "prairie", "summit",
];

const LEASE_ID_GENERATION_ATTEMPTS: usize = 16;

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
    leased: i64,
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
    let row = sqlx::query_as!(
        IssueListRow,
        r#"
        SELECT
            r.name AS "repo!: String",
            i.number AS "number!: i64", i.title AS "title!: String",
            i.body AS "body!: String", i.state AS "state!: String",
            COALESCE(s.status_type, 'unstarted') AS "status_type!: String",
            i.priority AS "priority!: i64",
            p.id AS "project_id?: i64", p.name AS "project_name?: String",
            m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
            EXISTS (
                SELECT 1 FROM issue_leases l
                WHERE l.repo_id = i.repo_id AND l.issue_number = i.number
            ) AS "leased!: i64",
            i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
            COALESCE(s.is_terminal, 0) AS "is_terminal!: i64",
            COALESCE(s.position, 0) AS "state_position!: i64"
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
        repo,
        number
    )
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|row| into_entry(row).issue))
}

pub async fn list_entries(pool: &SqlitePool, repo: Option<i64>) -> Result<Vec<IssueListEntry>> {
    let rows: Vec<IssueListRow> = match repo {
        Some(repo) => {
            sqlx::query_as!(
                IssueListRow,
                r#"
            SELECT
                r.name AS "repo!: String",
                i.number AS "number!: i64", i.title AS "title!: String",
                i.body AS "body!: String", i.state AS "state!: String",
                COALESCE(s.status_type, 'unstarted') AS "status_type!: String",
                i.priority AS "priority!: i64",
                p.id AS "project_id?: i64", p.name AS "project_name?: String",
                m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
                EXISTS (
                    SELECT 1 FROM issue_leases l
                    WHERE l.repo_id = i.repo_id AND l.issue_number = i.number
                ) AS "leased!: i64",
                i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
                COALESCE(s.is_terminal, 0) AS "is_terminal!: i64",
                COALESCE(s.position, 0) AS "state_position!: i64"
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
                i.number AS "number!: i64", i.title AS "title!: String",
                i.body AS "body!: String", i.state AS "state!: String",
                COALESCE(s.status_type, 'unstarted') AS "status_type!: String",
                i.priority AS "priority!: i64",
                p.id AS "project_id?: i64", p.name AS "project_name?: String",
                m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
                EXISTS (
                    SELECT 1 FROM issue_leases l
                    WHERE l.repo_id = i.repo_id AND l.issue_number = i.number
                ) AS "leased!: i64",
                i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
                COALESCE(s.is_terminal, 0) AS "is_terminal!: i64",
                COALESCE(s.position, 0) AS "state_position!: i64"
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
            leased: row.leased != 0,
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
    Ok(sqlx::query_scalar!(
        r#"SELECT CASE
               WHEN low_number = ? THEN high_number
               ELSE low_number
           END AS "number!: i64"
           FROM issue_relations
           WHERE repo_id = ? AND (low_number = ? OR high_number = ?)
           ORDER BY 1"#,
        number,
        repo,
        number,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn linked_prs(pool: &SqlitePool, repo: i64, number: i64) -> Result<Vec<PrRef>> {
    Ok(sqlx::query!(
        r#"SELECT p.number AS "number!: i64",
                  p.title AS "title!: String",
                  p.branch AS "branch!: String",
                  p.state AS "state!: String"
           FROM issue_pr_links l
           JOIN prs p ON p.repo_id = l.repo_id AND p.number = l.pr_number
           WHERE l.repo_id = ? AND l.issue_number = ?
           ORDER BY p.number"#,
        repo,
        number
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| PrRef {
        number: row.number,
        title: row.title,
        branch: row.branch,
        state: row.state,
    })
    .collect())
}

pub async fn insert_relation(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    a: i64,
    b: i64,
) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query!(
        r#"INSERT INTO issue_relations (repo_id, low_number, high_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, low_number, high_number) DO NOTHING"#,
        repo,
        low,
        high
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn remove_relation(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    a: i64,
    b: i64,
) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query!(
        "DELETE FROM issue_relations WHERE repo_id = ? AND low_number = ? AND high_number = ?",
        repo,
        low,
        high
    )
    .execute(&mut **tx)
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

pub async fn set_project_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    project_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_projects (repo_id, issue_number, project_id)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
        repo,
        number,
        project_id
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repo, number).await
}

pub async fn clear_project_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_projects WHERE repo_id = ? AND issue_number = ?",
        repo,
        number
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repo, number).await
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
    lease: Option<&str>,
) -> Result<()> {
    let mut tx = begin_lease_mutation(pool, repo, child, lease).await?;
    if let Some(project_id) = inherited_project {
        sqlx::query!(
            r#"INSERT INTO issue_projects (repo_id, issue_number, project_id)
               VALUES (?, ?, ?)
               ON CONFLICT(repo_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
            repo,
            child,
            project_id
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        r#"INSERT INTO issue_parents (repo_id, child_number, parent_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repo_id, child_number) DO UPDATE SET parent_number = excluded.parent_number"#,
        repo,
        child,
        parent
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE issues SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        repo,
        child
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_parent_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    child: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_parents WHERE repo_id = ? AND child_number = ?",
        repo,
        child
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repo, child).await
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

pub async fn update_state(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    state: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET state = ?, updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        state,
        repo,
        number
    )
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn edit(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
    priority: Option<i64>,
) -> Result<()> {
    if let Some(title) = title {
        sqlx::query!(
            "UPDATE issues SET title = ? WHERE repo_id = ? AND number = ?",
            title,
            repo,
            number
        )
        .execute(&mut **tx)
        .await?;
    }
    if let Some(body) = body {
        sqlx::query!(
            "UPDATE issues SET body = ? WHERE repo_id = ? AND number = ?",
            body,
            repo,
            number
        )
        .execute(&mut **tx)
        .await?;
    }
    if let Some(priority) = priority {
        sqlx::query!(
            "UPDATE issues SET priority = ? WHERE repo_id = ? AND number = ?",
            priority,
            repo,
            number
        )
        .execute(&mut **tx)
        .await?;
    }
    touch_tx(tx, repo, number).await
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

pub async fn touch_tx(tx: &mut Transaction<'_, Sqlite>, repo: i64, number: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
        repo,
        number
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_dependency(
    tx: &mut Transaction<'_, Sqlite>,
    repo: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO issue_deps (repo_id, blocker_number, blocked_number) VALUES (?, ?, ?)",
        repo,
        blocker,
        blocked
    )
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn remove_dependency(
    tx: &mut Transaction<'_, Sqlite>,
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn acquire_lease(pool: &SqlitePool, repo: i64, number: i64) -> Result<Option<String>> {
    acquire_lease_with(pool, repo, number, random_lease_id).await
}

fn random_lease_id() -> String {
    format!(
        "{}-{}-{}",
        LEASE_ADJECTIVES[fastrand::usize(..LEASE_ADJECTIVES.len())],
        LEASE_ANIMALS[fastrand::usize(..LEASE_ANIMALS.len())],
        LEASE_OBJECTS[fastrand::usize(..LEASE_OBJECTS.len())]
    )
}

async fn acquire_lease_with(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    mut generate_id: impl FnMut() -> String,
) -> Result<Option<String>> {
    for _ in 0..LEASE_ID_GENERATION_ATTEMPTS {
        let lease_id = generate_id();
        let acquired = sqlx::query_scalar!(
            r#"INSERT INTO issue_leases (repo_id, issue_number, lease_id)
               SELECT ?, ?, ?
               WHERE EXISTS (
                   SELECT 1 FROM issues WHERE repo_id = ? AND number = ?
               )
               ON CONFLICT DO NOTHING
               RETURNING lease_id AS "lease_id!: String""#,
            repo,
            number,
            lease_id,
            repo,
            number
        )
        .fetch_optional(pool)
        .await?;
        if acquired.is_some() {
            return Ok(acquired);
        }

        let already_leased = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                   SELECT 1 FROM issue_leases
                   WHERE repo_id = ? AND issue_number = ?
               ) AS "leased!: bool""#,
            repo,
            number
        )
        .fetch_one(pool)
        .await?;
        if already_leased {
            return Ok(None);
        }
    }

    anyhow::bail!("could not generate a unique lease ID after repeated collisions")
}

/// Start a protected Issue mutation. The no-op UPDATE both validates the lease
/// and obtains SQLite's write lock, so a concurrent force unlock cannot land
/// between validation and the mutation committed by the returned transaction.
pub async fn begin_lease_mutation<'a>(
    pool: &'a SqlitePool,
    repo: i64,
    number: i64,
    lease: Option<&str>,
) -> Result<Transaction<'a, Sqlite>> {
    let lease = lease.ok_or_else(|| {
        anyhow::anyhow!(
            "valid lease required for issue #{number}; acquire one with `octa issue lock {number}`"
        )
    })?;
    let mut tx = pool.begin().await?;
    let matched = sqlx::query!(
        r#"UPDATE issue_leases SET lease_id = lease_id
           WHERE repo_id = ? AND issue_number = ? AND lease_id = ?"#,
        repo,
        number,
        lease
    )
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !matched {
        anyhow::bail!("valid lease required for issue #{number}");
    }
    Ok(tx)
}

pub async fn release_lease(
    pool: &SqlitePool,
    repo: i64,
    number: i64,
    lease: Option<&str>,
    force: bool,
) -> Result<bool> {
    let result = if force {
        sqlx::query!(
            "DELETE FROM issue_leases WHERE repo_id = ? AND issue_number = ?",
            repo,
            number
        )
        .execute(pool)
        .await?
    } else {
        let lease = lease.ok_or_else(|| anyhow::anyhow!("--lease is required without --force"))?;
        sqlx::query!(
            r#"DELETE FROM issue_leases
               WHERE repo_id = ? AND issue_number = ? AND lease_id = ?"#,
            repo,
            number,
            lease
        )
        .execute(pool)
        .await?
    };
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

#[cfg(test)]
mod tests {
    use super::acquire_lease_with;
    use sqlx::sqlite::SqlitePoolOptions;

    #[tokio::test]
    async fn lease_acquisition_retries_an_id_collision() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(include_str!("../../migrations/0001_init.sql"))
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO repos (id, identity_key, name) VALUES (1, 'test', 'test')")
            .execute(&pool)
            .await
            .unwrap();
        for number in [1_i64, 2] {
            sqlx::query(
                "INSERT INTO issues (repo_id, number, title, state) VALUES (1, ?, 'Issue', 'open')",
            )
            .bind(number)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO issue_leases (repo_id, issue_number, lease_id) VALUES (1, 1, 'amber-otter-lantern')"
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut candidates = ["amber-otter-lantern", "quiet-heron-summit"].into_iter();
        let acquired = acquire_lease_with(&pool, 1, 2, || candidates.next().unwrap().to_string())
            .await
            .unwrap();

        assert_eq!(acquired.as_deref(), Some("quiet-heron-summit"));
    }
}
