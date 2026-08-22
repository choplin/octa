//! SQLite repository operations for Issue. This module owns SQLx and converts
//! database rows to domain values immediately; workflow policy lives in app.

use crate::domain::issue::{Issue, IssueListEntry, IssueRef, IssueState, StateType};
use crate::domain::milestone::MilestoneRef;
use crate::domain::Comment;
use crate::domain::{project::ProjectRef, pull_request::PullRequestRef};
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
    repository: String,
    number: i64,
    title: String,
    body: String,
    state: String,
    project_id: Option<i64>,
    project_name: Option<String>,
    milestone_id: Option<i64>,
    milestone_name: Option<String>,
    leased: i64,
    created_at: String,
    updated_at: String,
    state_type: String,
}

pub async fn insert(
    pool: &SqlitePool,
    repository: i64,
    title: &str,
    body: &str,
    state: &str,
) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"INSERT INTO issues (repository_id, number, title, body, state)
           VALUES (?, (SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repository_id = ?), ?, ?, ?)
           RETURNING number AS "number!: i64""#,
        repository,
        repository,
        title,
        body,
        state
    )
    .fetch_one(pool)
    .await?)
}

pub async fn get(pool: &SqlitePool, repository: i64, number: i64) -> Result<Option<Issue>> {
    let row = sqlx::query_as!(
        IssueListRow,
        r#"
        SELECT
            r.name AS "repository!: String",
            i.number AS "number!: i64", i.title AS "title!: String",
            i.body AS "body!: String", i.state AS "state!: String",
            p.id AS "project_id?: i64", p.name AS "project_name?: String",
            m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
            EXISTS (
                SELECT 1 FROM issue_leases l
                WHERE l.repository_id = i.repository_id AND l.issue_number = i.number
            ) AS "leased!: i64",
            i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
            s.type AS "state_type!: String"
        FROM
            issues i
        JOIN
            repositories r ON r.id = i.repository_id
        JOIN
            issue_states s ON s.name = i.state
        LEFT JOIN issue_projects ip
            ON ip.repository_id = i.repository_id AND ip.issue_number = i.number
        LEFT JOIN projects p
            ON p.repository_id = ip.repository_id AND p.id = ip.project_id
        LEFT JOIN issue_milestones im
            ON im.repository_id = i.repository_id AND im.issue_number = i.number
        LEFT JOIN project_milestones m
            ON m.repository_id = im.repository_id AND m.project_id = im.project_id
               AND m.id = im.milestone_id
        WHERE
            i.repository_id = ?
        AND
            i.number = ?
    "#,
        repository,
        number
    )
    .fetch_optional(pool)
    .await?;
    row.map(|row| Ok(into_entry(row)?.issue)).transpose()
}

pub async fn list_entries(
    pool: &SqlitePool,
    repository: Option<i64>,
) -> Result<Vec<IssueListEntry>> {
    let rows: Vec<IssueListRow> = match repository {
        Some(repository) => {
            sqlx::query_as!(
                IssueListRow,
                r#"
            SELECT
                r.name AS "repository!: String",
                i.number AS "number!: i64", i.title AS "title!: String",
                i.body AS "body!: String", i.state AS "state!: String",
                p.id AS "project_id?: i64", p.name AS "project_name?: String",
                m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
                EXISTS (
                    SELECT 1 FROM issue_leases l
                    WHERE l.repository_id = i.repository_id AND l.issue_number = i.number
                ) AS "leased!: i64",
                i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
                s.type AS "state_type!: String"
            FROM
                issues i
            JOIN
                repositories r ON r.id = i.repository_id
            JOIN
                issue_states s ON s.name = i.state
            LEFT JOIN issue_projects ip
                ON ip.repository_id = i.repository_id AND ip.issue_number = i.number
            LEFT JOIN projects p
                ON p.repository_id = ip.repository_id AND p.id = ip.project_id
            LEFT JOIN issue_milestones im
                ON im.repository_id = i.repository_id AND im.issue_number = i.number
            LEFT JOIN project_milestones m
                ON m.repository_id = im.repository_id AND m.project_id = im.project_id
                   AND m.id = im.milestone_id
            WHERE
                i.repository_id = ?
            ORDER BY
                i.number
        "#,
                repository
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                IssueListRow,
                r#"
            SELECT
                r.name AS "repository!: String",
                i.number AS "number!: i64", i.title AS "title!: String",
                i.body AS "body!: String", i.state AS "state!: String",
                p.id AS "project_id?: i64", p.name AS "project_name?: String",
                m.id AS "milestone_id?: i64", m.name AS "milestone_name?: String",
                EXISTS (
                    SELECT 1 FROM issue_leases l
                    WHERE l.repository_id = i.repository_id AND l.issue_number = i.number
                ) AS "leased!: i64",
                i.created_at AS "created_at!: String", i.updated_at AS "updated_at!: String",
                s.type AS "state_type!: String"
            FROM
                issues i
            JOIN
                repositories r ON r.id = i.repository_id
            JOIN
                issue_states s ON s.name = i.state
            LEFT JOIN issue_projects ip
                ON ip.repository_id = i.repository_id AND ip.issue_number = i.number
            LEFT JOIN projects p
                ON p.repository_id = ip.repository_id AND p.id = ip.project_id
            LEFT JOIN issue_milestones im
                ON im.repository_id = i.repository_id AND im.issue_number = i.number
            LEFT JOIN project_milestones m
                ON m.repository_id = im.repository_id AND m.project_id = im.project_id
                   AND m.id = im.milestone_id
            ORDER BY
                r.name, i.number
        "#,
            )
            .fetch_all(pool)
            .await?
        }
    };
    rows.into_iter().map(into_entry).collect()
}

fn into_entry(row: IssueListRow) -> Result<IssueListEntry> {
    let state_type = state_type_of(&row.state_type)?;
    Ok(IssueListEntry {
        issue: Issue {
            repository: row.repository,
            number: row.number,
            title: row.title,
            body: row.body,
            state: row.state,
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
        state_type,
    })
}

/// Classify a `type` column read back from the database.
///
/// The column is constrained to the three known values, and every issue joins
/// to a configured state, so anything else means the database was written
/// outside octa. Report it rather than guessing a type on the caller's behalf.
fn state_type_of(value: &str) -> Result<StateType> {
    StateType::parse(value)
}

pub async fn labelled_numbers(
    pool: &SqlitePool,
    repository: i64,
    label: &str,
) -> Result<HashSet<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT issue_number AS "n!: i64" FROM issue_labels WHERE repository_id = ? AND label_name = ?"#, repository, label).fetch_all(pool).await?.into_iter().collect())
}

/// Whether each issue in the repository sits in a closed-type state.
pub async fn closed_flags(pool: &SqlitePool, repository: i64) -> Result<Vec<(i64, bool)>> {
    sqlx::query!(r#"SELECT i.number AS "number!: i64", s.type AS "state_type!: String" FROM issues i JOIN issue_states s ON s.name = i.state WHERE i.repository_id = ?"#, repository).fetch_all(pool).await?.into_iter().map(|row| Ok((row.number, state_type_of(&row.state_type)?.is_closed()))).collect()
}

pub async fn dependencies(pool: &SqlitePool, repository: i64) -> Result<Vec<(i64, i64)>> {
    Ok(sqlx::query!(r#"SELECT blocker_number AS "blocker!: i64", blocked_number AS "blocked!: i64" FROM issue_dependencies WHERE repository_id = ?"#, repository).fetch_all(pool).await?.into_iter().map(|row| (row.blocker, row.blocked)).collect())
}

pub async fn comments(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<Comment>> {
    Ok(sqlx::query_as!(Comment, r#"SELECT id AS "id!: i64", body AS "body!: String", created_at AS "created_at!: String" FROM issue_comments WHERE repository_id = ? AND issue_number = ? ORDER BY id"#, repository, number).fetch_all(pool).await?)
}

pub async fn labels(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<String>> {
    Ok(sqlx::query_scalar!(r#"SELECT label_name AS "l!: String" FROM issue_labels WHERE repository_id = ? AND issue_number = ? ORDER BY label_name"#, repository, number).fetch_all(pool).await?)
}

pub async fn blocks(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT blocked_number AS "n!: i64" FROM issue_dependencies WHERE repository_id = ? AND blocker_number = ? ORDER BY blocked_number"#, repository, number).fetch_all(pool).await?)
}

pub async fn blocked_by(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(r#"SELECT blocker_number AS "n!: i64" FROM issue_dependencies WHERE repository_id = ? AND blocked_number = ? ORDER BY blocker_number"#, repository, number).fetch_all(pool).await?)
}

pub async fn related(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<i64>> {
    Ok(sqlx::query_scalar!(
        r#"SELECT CASE
               WHEN low_number = ? THEN high_number
               ELSE low_number
           END AS "number!: i64"
           FROM issue_relations
           WHERE repository_id = ? AND (low_number = ? OR high_number = ?)
           ORDER BY 1"#,
        number,
        repository,
        number,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn linked_pull_requests(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
) -> Result<Vec<PullRequestRef>> {
    Ok(sqlx::query!(
        r#"SELECT p.number AS "number!: i64",
                  p.title AS "title!: String",
                  p.branch AS "branch!: String",
                  p.state AS "state!: String"
           FROM issue_pull_request_links l
           JOIN pull_requests p ON p.repository_id = l.repository_id AND p.number = l.pull_request_number
           WHERE l.repository_id = ? AND l.issue_number = ?
           ORDER BY p.number"#,
        repository,
        number
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| PullRequestRef {
        number: row.number,
        title: row.title,
        branch: row.branch,
        state: row.state,
    })
    .collect())
}

pub async fn insert_relation(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    a: i64,
    b: i64,
) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query!(
        r#"INSERT INTO issue_relations (repository_id, low_number, high_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, low_number, high_number) DO NOTHING"#,
        repository,
        low,
        high
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn remove_relation(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    a: i64,
    b: i64,
) -> Result<()> {
    let (low, high) = if a < b { (a, b) } else { (b, a) };
    sqlx::query!(
        "DELETE FROM issue_relations WHERE repository_id = ? AND low_number = ? AND high_number = ?",
        repository,
        low,
        high
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn parent(pool: &SqlitePool, repository: i64, number: i64) -> Result<Option<IssueRef>> {
    Ok(sqlx::query_as!(
        IssueRef,
        r#"SELECT i.number AS "number!: i64", i.title AS "title!: String"
           FROM issue_parents p
           JOIN issues i ON i.repository_id = p.repository_id AND i.number = p.parent_number
           WHERE p.repository_id = ? AND p.child_number = ?"#,
        repository,
        number
    )
    .fetch_optional(pool)
    .await?)
}

pub async fn children(pool: &SqlitePool, repository: i64, number: i64) -> Result<Vec<IssueRef>> {
    Ok(sqlx::query_as!(
        IssueRef,
        r#"SELECT i.number AS "number!: i64", i.title AS "title!: String"
           FROM issue_parents p
           JOIN issues i ON i.repository_id = p.repository_id AND i.number = p.child_number
           WHERE p.repository_id = ? AND p.parent_number = ?
           ORDER BY i.number"#,
        repository,
        number
    )
    .fetch_all(pool)
    .await?)
}

pub async fn set_project(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    project_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_projects (repository_id, issue_number, project_id)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
        repository,
        number,
        project_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_project_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    project_id: i64,
) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_projects (repository_id, issue_number, project_id)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
        repository,
        number,
        project_id
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repository, number).await
}

pub async fn clear_project_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_projects WHERE repository_id = ? AND issue_number = ?",
        repository,
        number
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repository, number).await
}

pub async fn set_parent(pool: &SqlitePool, repository: i64, child: i64, parent: i64) -> Result<()> {
    sqlx::query!(
        r#"INSERT INTO issue_parents (repository_id, child_number, parent_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, child_number) DO UPDATE SET parent_number = excluded.parent_number"#,
        repository,
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
    repository: i64,
    child: i64,
    parent: i64,
    inherited_project: Option<i64>,
    lease: Option<&str>,
) -> Result<()> {
    let mut tx = begin_lease_mutation(pool, repository, child, lease).await?;
    if let Some(project_id) = inherited_project {
        sqlx::query!(
            r#"INSERT INTO issue_projects (repository_id, issue_number, project_id)
               VALUES (?, ?, ?)
               ON CONFLICT(repository_id, issue_number) DO UPDATE SET project_id = excluded.project_id"#,
            repository,
            child,
            project_id
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!(
        r#"INSERT INTO issue_parents (repository_id, child_number, parent_number)
           VALUES (?, ?, ?)
           ON CONFLICT(repository_id, child_number) DO UPDATE SET parent_number = excluded.parent_number"#,
        repository,
        child,
        parent
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query!(
        "UPDATE issues SET updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        repository,
        child
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn clear_parent_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    child: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_parents WHERE repository_id = ? AND child_number = ?",
        repository,
        child
    )
    .execute(&mut **tx)
    .await?;
    touch_tx(tx, repository, child).await
}

pub async fn insert_comment(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
    body: &str,
) -> Result<()> {
    sqlx::query!(
        "INSERT INTO issue_comments (repository_id, issue_number, body) VALUES (?, ?, ?)",
        repository,
        number,
        body
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn state_exists(pool: &SqlitePool, state: &str) -> Result<bool> {
    Ok(
        sqlx::query_scalar!("SELECT COUNT(*) FROM issue_states WHERE name = ?", state)
            .fetch_one(pool)
            .await?
            != 0,
    )
}

pub async fn update_state(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    state: &str,
) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET state = ?, updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        state,
        repository,
        number
    )
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn edit(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<()> {
    if let Some(title) = title {
        sqlx::query!(
            "UPDATE issues SET title = ? WHERE repository_id = ? AND number = ?",
            title,
            repository,
            number
        )
        .execute(&mut **tx)
        .await?;
    }
    if let Some(body) = body {
        sqlx::query!(
            "UPDATE issues SET body = ? WHERE repository_id = ? AND number = ?",
            body,
            repository,
            number
        )
        .execute(&mut **tx)
        .await?;
    }
    touch_tx(tx, repository, number).await
}

pub async fn touch(pool: &SqlitePool, repository: i64, number: i64) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        repository,
        number
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn touch_tx(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    number: i64,
) -> Result<()> {
    sqlx::query!(
        "UPDATE issues SET updated_at = datetime('now') WHERE repository_id = ? AND number = ?",
        repository,
        number
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn insert_dependency(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    sqlx::query!(
        "INSERT OR IGNORE INTO issue_dependencies (repository_id, blocker_number, blocked_number) VALUES (?, ?, ?)",
        repository,
        blocker,
        blocked
    )
        .execute(&mut **tx)
        .await?;
    Ok(())
}

pub async fn remove_dependency(
    tx: &mut Transaction<'_, Sqlite>,
    repository: i64,
    blocker: i64,
    blocked: i64,
) -> Result<()> {
    sqlx::query!(
        "DELETE FROM issue_dependencies WHERE repository_id = ? AND blocker_number = ? AND blocked_number = ?",
        repository,
        blocker,
        blocked
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub async fn acquire_lease(
    pool: &SqlitePool,
    repository: i64,
    number: i64,
) -> Result<Option<String>> {
    acquire_lease_with(pool, repository, number, random_lease_id).await
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
    repository: i64,
    number: i64,
    mut generate_id: impl FnMut() -> String,
) -> Result<Option<String>> {
    for _ in 0..LEASE_ID_GENERATION_ATTEMPTS {
        let lease_id = generate_id();
        let acquired = sqlx::query_scalar!(
            r#"INSERT INTO issue_leases (repository_id, issue_number, lease_id)
               SELECT ?, ?, ?
               WHERE EXISTS (
                   SELECT 1 FROM issues WHERE repository_id = ? AND number = ?
               )
               ON CONFLICT DO NOTHING
               RETURNING lease_id AS "lease_id!: String""#,
            repository,
            number,
            lease_id,
            repository,
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
                   WHERE repository_id = ? AND issue_number = ?
               ) AS "leased!: bool""#,
            repository,
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
    repository: i64,
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
           WHERE repository_id = ? AND issue_number = ? AND lease_id = ?"#,
        repository,
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
    repository: i64,
    number: i64,
    lease: Option<&str>,
    force: bool,
) -> Result<bool> {
    let result = if force {
        sqlx::query!(
            "DELETE FROM issue_leases WHERE repository_id = ? AND issue_number = ?",
            repository,
            number
        )
        .execute(pool)
        .await?
    } else {
        let lease = lease.ok_or_else(|| anyhow::anyhow!("--lease is required without --force"))?;
        sqlx::query!(
            r#"DELETE FROM issue_leases
               WHERE repository_id = ? AND issue_number = ? AND lease_id = ?"#,
            repository,
            number,
            lease
        )
        .execute(pool)
        .await?
    };
    Ok(result.rows_affected() == 1)
}

/// Seed the default state set, but only when no state is configured at all.
///
/// States are global, so this runs once for the store rather than once per
/// repository. A store whose states were already customized keeps exactly the
/// set it has.
///
/// The states and their defaults go in together, in one transaction: a store
/// with states but no default would leave every verb with nowhere to go. The
/// defaults are named outright rather than inferred from insertion order --
/// order is not a property the schema preserves, so anything that replays these
/// rows in a different order would silently pick different defaults.
pub async fn seed_default_states(pool: &SqlitePool) -> Result<()> {
    let mut tx = pool.begin().await?;
    let configured = sqlx::query_scalar!(r#"SELECT COUNT(*) AS "count!: i64" FROM issue_states"#)
        .fetch_one(&mut *tx)
        .await?;
    if configured != 0 {
        return Ok(());
    }
    // One state per type, plus the second way work ends. `not planned` is a
    // reason rather than a fourth type, so it shares the closed type with
    // `closed` instead of extending the axis. Names are lower case to match
    // `pull_requests.state` and the GitHub API's own value spelling.
    for (state, state_type) in [
        ("open", "open"),
        ("in progress", "in progress"),
        ("closed", "closed"),
        ("not planned", "closed"),
    ] {
        sqlx::query!(
            "INSERT INTO issue_states (name, type) VALUES (?, ?)",
            state,
            state_type
        )
        .execute(&mut *tx)
        .await?;
    }
    for (state_type, state) in [
        ("open", "open"),
        ("in progress", "in progress"),
        ("closed", "closed"),
    ] {
        sqlx::query!(
            "INSERT INTO issue_state_defaults (type, name) VALUES (?, ?)",
            state_type,
            state
        )
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// List every configured state in lifecycle order.
///
/// States carry no stored ordinal, so the order is derived: by type, then the
/// type's default first, then by name.
pub async fn list_states(pool: &SqlitePool) -> Result<Vec<IssueState>> {
    sqlx::query!(r#"SELECT s.name AS "name!: String", s.type AS "state_type!: String", (d.name IS NOT NULL) AS "is_default!: i64" FROM issue_states s LEFT JOIN issue_state_defaults d ON d.name = s.name ORDER BY CASE s.type WHEN 'open' THEN 0 WHEN 'in progress' THEN 1 ELSE 2 END, (d.name IS NOT NULL) DESC, s.name"#)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|row| {
            Ok(IssueState {
                name: row.name,
                state_type: state_type_of(&row.state_type)?,
                is_default: row.is_default != 0,
            })
        })
        .collect()
}

/// Create a state, optionally taking its type's default.
///
/// Both writes share one transaction, so a state that is meant to be its type's
/// default never exists without being one. Nothing in the schema hands the
/// default out on its own; the caller decides, which is what keeps a restored
/// dump's defaults intact.
pub async fn insert_state(
    pool: &SqlitePool,
    name: &str,
    state_type: StateType,
    default: bool,
) -> Result<()> {
    let type_value = state_type.as_str();
    let mut tx = pool.begin().await?;
    sqlx::query!(
        "INSERT INTO issue_states (name, type) VALUES (?, ?)",
        name,
        type_value
    )
    .execute(&mut *tx)
    .await?;
    if default {
        set_default_state_tx(&mut tx, name, state_type).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// The state `state_type` hands out when a verb is given no explicit target.
///
/// The default is keyed by type, so this is a primary-key lookup returning one
/// state or none. None means the type has no states at all.
pub async fn default_state(pool: &SqlitePool, state_type: StateType) -> Result<Option<String>> {
    let type_value = state_type.as_str();
    Ok(sqlx::query_scalar!(
        "SELECT name FROM issue_state_defaults WHERE type = ?",
        type_value
    )
    .fetch_optional(pool)
    .await?)
}

/// Every configured state of one type, in listing order.
pub async fn states_of_type(pool: &SqlitePool, state_type: StateType) -> Result<Vec<String>> {
    let type_value = state_type.as_str();
    Ok(sqlx::query_scalar!(
        "SELECT s.name FROM issue_states s LEFT JOIN issue_state_defaults d ON d.name = s.name WHERE s.type = ? ORDER BY (d.name IS NOT NULL) DESC, s.name",
        type_value
    )
    .fetch_all(pool)
    .await?)
}

pub async fn get_state(pool: &SqlitePool, name: &str) -> Result<Option<IssueState>> {
    sqlx::query!(r#"SELECT s.name AS "name!: String", s.type AS "state_type!: String", (d.name IS NOT NULL) AS "is_default!: i64" FROM issue_states s LEFT JOIN issue_state_defaults d ON d.name = s.name WHERE s.name = ?"#, name)
        .fetch_optional(pool)
        .await?
        .map(|row| {
            Ok(IssueState {
                name: row.name,
                state_type: state_type_of(&row.state_type)?,
                is_default: row.is_default != 0,
            })
        })
        .transpose()
}

/// Count issues in a state across every repository.
///
/// States are global, so deleting or renaming one reaches every repository's
/// issues, not just the active one.
pub async fn count_issues_in_state(pool: &SqlitePool, name: &str) -> Result<i64> {
    Ok(sqlx::query_scalar!(
        r#"SELECT COUNT(*) AS "count!: i64" FROM issues WHERE state = ?"#,
        name
    )
    .fetch_one(pool)
    .await?)
}

/// Rename a state.
///
/// `issues.state` references the name with `ON UPDATE CASCADE`, so every issue
/// in the state follows in the same statement, across every repository.
pub async fn rename_state(pool: &SqlitePool, from: &str, to: &str) -> Result<()> {
    sqlx::query!("UPDATE issue_states SET name = ? WHERE name = ?", to, from)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a state, first moving any issues that reference it to `move_to`.
///
/// The state is global, so this reaches every repository's issues. Both
/// statements share one transaction because `ON DELETE RESTRICT` rejects the
/// delete until the last issue has left the state.
pub async fn delete_state(pool: &SqlitePool, name: &str, move_to: Option<&str>) -> Result<()> {
    let mut tx = pool.begin().await?;
    if let Some(move_to) = move_to {
        sqlx::query!(
            "UPDATE issues SET state = ?, updated_at = datetime('now') WHERE state = ?",
            move_to,
            name
        )
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query!("DELETE FROM issue_states WHERE name = ?", name)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(())
}

/// Make `name` the default state of its own type.
///
/// One row per type means one write: the previous default is replaced in place
/// rather than cleared and re-set, so the type is never momentarily without one.
pub async fn set_default_state(pool: &SqlitePool, name: &str, state_type: StateType) -> Result<()> {
    let mut tx = pool.begin().await?;
    set_default_state_tx(&mut tx, name, state_type).await?;
    tx.commit().await?;
    Ok(())
}

async fn set_default_state_tx(
    tx: &mut Transaction<'_, Sqlite>,
    name: &str,
    state_type: StateType,
) -> Result<()> {
    let type_value = state_type.as_str();
    sqlx::query!(
        "INSERT INTO issue_state_defaults (type, name) VALUES (?, ?)
         ON CONFLICT(type) DO UPDATE SET name = excluded.name",
        type_value,
        name
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
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
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, 'test', 'test')")
            .execute(&pool)
            .await
            .unwrap();
        super::seed_default_states(&pool).await.unwrap();
        for number in [1_i64, 2] {
            sqlx::query(
                "INSERT INTO issues (repository_id, number, title, state) VALUES (1, ?, 'Issue', 'open')",
            )
            .bind(number)
            .execute(&pool)
            .await
            .unwrap();
        }
        sqlx::query(
            "INSERT INTO issue_leases (repository_id, issue_number, lease_id) VALUES (1, 1, 'amber-otter-lantern')"
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
