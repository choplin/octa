//! Storage layer for octa: a single global SQLite database under the XDG data
//! directory holds every entity for every repository. Entity rows carry a
//! `repo_id`, so the store is physically central but logically scoped per
//! repository — the current repository by default (resolved from cwd via the
//! git common directory), any named repository, or all of them at once.
//! Configuration — issue states, labels, and label groups — is global instead:
//! one set governs every repository.
//!
//! Repo identity is the canonicalized git common directory path (works without
//! a remote; every worktree of a repo shares it). Concurrency rides on SQLite
//! WAL plus a busy timeout; per-repo issue/PR numbers are assigned atomically by
//! a single `INSERT ... RETURNING`, and issue leases are acquired with a
//! compare-and-set.

mod issue;
mod label;
mod milestone;
mod pr;
mod project;
mod wiki;

pub use crate::domain::{
    issue::{IssueListSelector, LeaseOutcome, StateType},
    StateFilter,
};

use anyhow::{bail, Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// Which repositories an operation targets.
#[derive(Debug, Clone)]
pub enum RepoScope {
    /// The repository containing the current working directory (default).
    Current,
    /// A named repository already known to the store.
    Named(String),
    /// Every repository (read/aggregate operations only).
    All,
}

enum Resolved {
    One(i64),
    All,
}

/// Resolve the identity key and human name of the repository at the cwd.
///
/// The key is the canonicalized `git rev-parse --git-common-dir` path, shared by
/// every worktree of the same repository; the name is the repository root's
/// directory basename.
fn resolve_repo_identity() -> Result<(String, String)> {
    let common = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .context("failed to run `git rev-parse --git-common-dir`")?;
    if !common.status.success() {
        bail!("not inside a git repository (git rev-parse --git-common-dir failed)");
    }
    let raw = String::from_utf8(common.stdout)
        .context("git printed non-UTF-8 output")?
        .trim()
        .to_string();
    let mut git_dir = PathBuf::from(&raw);
    if git_dir.is_relative() {
        git_dir = std::env::current_dir()?.join(git_dir);
    }
    let git_dir = std::fs::canonicalize(&git_dir)
        .with_context(|| format!("cannot resolve git dir {}", git_dir.display()))?;
    let identity_key = git_dir.to_string_lossy().to_string();

    // Name: the repository root's basename. `--show-toplevel` gives the current
    // worktree's root; its parent-of-.git works even for bare-ish layouts.
    let name = repo_name(&git_dir);
    Ok((identity_key, name))
}

/// Derive a friendly repo name from the common git dir path (its parent's
/// basename, i.e. the repository directory), falling back to "repo".
fn repo_name(git_dir: &std::path::Path) -> String {
    git_dir
        .parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string())
}

/// Resolve the path to the single global database under the XDG data directory.
pub fn resolve_db_path() -> Result<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => {
            let home = std::env::var_os("HOME").context("neither XDG_DATA_HOME nor HOME is set")?;
            PathBuf::from(home).join(".local").join("share")
        }
    };
    let dir = base.join("octa");
    std::fs::create_dir_all(&dir).with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir.join("octa.db"))
}

pub struct Store {
    pub(crate) pool: SqlitePool,
    scope: Resolved,
}

impl Store {
    /// Open the global database (creating it if needed), apply migrations, and
    /// resolve the requested scope.
    pub async fn open(scope: RepoScope) -> Result<Self> {
        let path = resolve_db_path()?;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .with_context(|| format!("cannot open database at {}", path.display()))?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("cannot apply database migrations")?;
        // Configuration is global, so the default state set is seeded once for
        // the store rather than once per repository.
        crate::sql::issue::seed_default_states(&pool).await?;

        let store = Self {
            pool,
            scope: Resolved::All,
        };
        let scope = match scope {
            RepoScope::Current => {
                let (key, name) = resolve_repo_identity()?;
                Resolved::One(store.upsert_repo(&key, &name).await?)
            }
            RepoScope::Named(name) => Resolved::One(store.repo_by_name(&name).await?),
            RepoScope::All => Resolved::All,
        };
        Ok(Self {
            pool: store.pool,
            scope,
        })
    }

    /// The active single repo, or an error when the scope is `--all-repos`.
    pub(crate) fn repo_id(&self) -> Result<i64> {
        match self.scope {
            Resolved::One(id) => Ok(id),
            Resolved::All => bail!("this command needs a single repository; drop --all-repos"),
        }
    }

    pub(crate) fn is_all(&self) -> bool {
        matches!(self.scope, Resolved::All)
    }

    /// Insert the repo if new and return its id.
    async fn upsert_repo(&self, identity_key: &str, name: &str) -> Result<i64> {
        crate::sql::repo::upsert(&self.pool, identity_key, name).await
    }

    async fn repo_by_name(&self, name: &str) -> Result<i64> {
        crate::sql::repo::by_name(&self.pool, name).await
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{Resolved, StateType, Store};

    /// Move an issue through a lease-protected verb, acquiring and releasing the
    /// lease around it. `None` starts the issue; `Some` closes it, optionally
    /// into a named closed state.
    async fn transition(store: &Store, number: i64, close: Option<(&str, Option<&str>)>) {
        let lease = match store.lock_issue(number).await.unwrap() {
            crate::domain::issue::LeaseOutcome::Acquired(lease) => lease,
            crate::domain::issue::LeaseOutcome::AlreadyLeased => panic!("unexpected lease"),
        };
        match close {
            None => store.start_issue(number, Some(&lease)).await.unwrap(),
            Some((_, as_state)) => store
                .close_issue(number, as_state, Some(&lease))
                .await
                .unwrap(),
        };
        store
            .unlock_issue(number, Some(&lease), false)
            .await
            .unwrap();
    }

    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    #[derive(Debug, Eq, PartialEq)]
    struct LogicalSnapshot(Vec<(&'static str, Vec<String>)>);

    async fn logical_snapshot(pool: &SqlitePool) -> LogicalSnapshot {
        macro_rules! rows {
            ($query:literal) => {
                sqlx::query_scalar!($query).fetch_all(pool).await.unwrap()
            };
        }

        LogicalSnapshot(vec![
            (
                "repos",
                rows!(
                    r#"SELECT json_array(id, identity_key, name, created_at) AS "row!: String" FROM repos ORDER BY id"#
                ),
            ),
            (
                "issue_states",
                rows!(
                    r#"SELECT json_array(name, type) AS "row!: String" FROM issue_states ORDER BY name"#
                ),
            ),
            (
                "issue_state_defaults",
                rows!(
                    r#"SELECT json_array(type, name) AS "row!: String" FROM issue_state_defaults ORDER BY type"#
                ),
            ),
            (
                "issues",
                rows!(
                    r#"SELECT json_array(repo_id, number, title, body, state, created_at, updated_at) AS "row!: String" FROM issues ORDER BY repo_id, number"#
                ),
            ),
            (
                "issue_leases",
                rows!(
                    r#"SELECT json_array(repo_id, issue_number, lease_id, acquired_at) AS "row!: String" FROM issue_leases ORDER BY repo_id, issue_number"#
                ),
            ),
            (
                "comments",
                rows!(
                    r#"SELECT json_array(id, repo_id, issue_number, body, created_at) AS "row!: String" FROM comments ORDER BY id"#
                ),
            ),
            (
                "issue_deps",
                rows!(
                    r#"SELECT json_array(repo_id, blocker_number, blocked_number, created_at) AS "row!: String" FROM issue_deps ORDER BY repo_id, blocker_number, blocked_number"#
                ),
            ),
            (
                "prs",
                rows!(
                    r#"SELECT json_array(repo_id, number, title, body, branch, state, created_at, updated_at) AS "row!: String" FROM prs ORDER BY repo_id, number"#
                ),
            ),
            (
                "pr_comments",
                rows!(
                    r#"SELECT json_array(id, repo_id, pr_number, body, created_at) AS "row!: String" FROM pr_comments ORDER BY id"#
                ),
            ),
            (
                "wiki_pages",
                rows!(
                    r#"SELECT json_array(repo_id, slug, title, body, created_at, updated_at) AS "row!: String" FROM wiki_pages ORDER BY repo_id, slug"#
                ),
            ),
            (
                "wiki_links",
                rows!(
                    r#"SELECT json_array(repo_id, from_slug, to_slug) AS "row!: String" FROM wiki_links ORDER BY repo_id, from_slug, to_slug"#
                ),
            ),
            (
                "label_groups",
                rows!(
                    r#"SELECT json_array(name, selection) AS "row!: String" FROM label_groups ORDER BY name"#
                ),
            ),
            (
                "labels",
                rows!(
                    r#"SELECT json_array(name, group_name) AS "row!: String" FROM labels ORDER BY name"#
                ),
            ),
            (
                "issue_labels",
                rows!(
                    r#"SELECT json_array(repo_id, issue_number, label_name) AS "row!: String" FROM issue_labels ORDER BY repo_id, issue_number, label_name"#
                ),
            ),
            (
                "project_label_groups",
                rows!(
                    r#"SELECT json_array(name, selection) AS "row!: String" FROM project_label_groups ORDER BY name"#
                ),
            ),
            (
                "project_labels",
                rows!(
                    r#"SELECT json_array(name, group_name) AS "row!: String" FROM project_labels ORDER BY name"#
                ),
            ),
            (
                "projects",
                rows!(
                    r#"SELECT json_array(repo_id, id, name, summary, description, state, is_closed, created_at, updated_at) AS "row!: String" FROM projects ORDER BY repo_id, id"#
                ),
            ),
            (
                "project_label_links",
                rows!(
                    r#"SELECT json_array(repo_id, project_id, label_name) AS "row!: String" FROM project_label_links ORDER BY repo_id, project_id, label_name"#
                ),
            ),
            (
                "issue_projects",
                rows!(
                    r#"SELECT json_array(repo_id, issue_number, project_id) AS "row!: String" FROM issue_projects ORDER BY repo_id, issue_number"#
                ),
            ),
            (
                "issue_parents",
                rows!(
                    r#"SELECT json_array(repo_id, child_number, parent_number, created_at) AS "row!: String" FROM issue_parents ORDER BY repo_id, child_number"#
                ),
            ),
            (
                "issue_relations",
                rows!(
                    r#"SELECT json_array(repo_id, low_number, high_number, created_at) AS "row!: String" FROM issue_relations ORDER BY repo_id, low_number, high_number"#
                ),
            ),
            (
                "issue_pr_links",
                rows!(
                    r#"SELECT json_array(repo_id, issue_number, pr_number, created_at) AS "row!: String" FROM issue_pr_links ORDER BY repo_id, issue_number"#
                ),
            ),
        ])
    }

    #[tokio::test]
    async fn tui_is_read_only_after_store_initialization() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        let repo = crate::sql::repo::upsert(&pool, "/test/.git", "test")
            .await
            .unwrap();
        let store = Store {
            pool,
            scope: Resolved::One(repo),
        };
        // open, in progress, closed, and not planned come from the seed. Issues
        // reach the non-open types through the verbs, which is the only way in.
        let first = store
            .create_issue("First", "first body", None, None, None, None)
            .await
            .unwrap();
        let second = store
            .create_issue("Second", "second body", None, None, None, None)
            .await
            .unwrap();
        transition(&store, second, None).await;
        let review = store
            .create_issue("Review", "review body", None, None, None, None)
            .await
            .unwrap();
        transition(&store, review, None).await;
        let done = store
            .create_issue("Done", "done body", None, None, None, None)
            .await
            .unwrap();
        transition(&store, done, Some(("closed", None))).await;
        let canceled = store
            .create_issue("Not planned", "not planned body", None, None, None, None)
            .await
            .unwrap();
        transition(&store, canceled, Some(("closed", Some("not planned")))).await;
        // A state left over from an older workflow, still referenced by issues.
        store
            .add_state("archived", StateType::Closed, false)
            .await
            .unwrap();
        let legacy_closed = store
            .create_issue("Legacy closed", "legacy body", None, None, None, None)
            .await
            .unwrap();
        transition(&store, legacy_closed, Some(("closed", Some("archived")))).await;
        store
            .add_issue_comment(second, "still read-only")
            .await
            .unwrap();
        let lease = match store.lock_issue(first).await.unwrap() {
            crate::domain::issue::LeaseOutcome::Acquired(lease) => lease,
            crate::domain::issue::LeaseOutcome::AlreadyLeased => panic!("unexpected lease"),
        };
        store
            .add_dependency(first, first, second, Some(&lease))
            .await
            .unwrap();
        store
            .unlock_issue(first, Some(&lease), false)
            .await
            .unwrap();

        // Store migrations, repo/state seeding, and fixture writes are complete
        // before the baseline. WAL/checkpoint bytes are intentionally ignored.
        let before = logical_snapshot(&store.pool).await;
        let details = store.list_all_issue_details().await.unwrap();
        assert_eq!(
            details
                .iter()
                .map(|detail| detail.issue.number)
                .collect::<Vec<_>>(),
            vec![first, second, review, done, canceled, legacy_closed]
        );
        crate::tui::exercise_view_for_test(details);
        crate::sql::repo::upsert(&store.pool, "/test/.git", "test")
            .await
            .unwrap();
        let after = logical_snapshot(&store.pool).await;

        assert_eq!(after, before);
    }

    #[tokio::test]
    async fn fresh_schema_contains_the_current_model() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("./migrations");
        assert_eq!(migrator.migrations.len(), 1);
        migrator.run(&pool).await.unwrap();

        // Runtime-checked: `PRAGMA table_info` types no longer resolve offline
        // now that the table declares no column defaults.
        let state_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('issue_states')")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(state_columns, ["name", "type"]);

        // The invariant the schema holds is "a type with any states has exactly
        // one default, and that default is a state of that same type". Each
        // clause below is checked against the database, not the app layer.
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        // Runtime-checked queries: these assert schema behavior and have no
        // place in the offline query cache the application's own queries use.
        let seeded: Vec<(String, String)> =
            sqlx::query_as("SELECT type, name FROM issue_state_defaults ORDER BY type")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            seeded,
            vec![
                ("closed".to_string(), "closed".to_string()),
                ("in progress".to_string(), "in progress".to_string()),
                ("open".to_string(), "open".to_string()),
            ],
            "the first state of each type must become that type's default"
        );

        let unknown_type =
            sqlx::query("INSERT INTO issue_states (name, type) VALUES ('blocked', 'waiting')")
                .execute(&pool)
                .await;
        assert!(
            unknown_type.is_err(),
            "a type outside the three-value axis must be rejected by the schema"
        );

        // A second state of a populated type does not take the default.
        sqlx::query("INSERT INTO issue_states (name, type) VALUES ('triage', 'open')")
            .execute(&pool)
            .await
            .unwrap();
        let open_default: String =
            sqlx::query_scalar("SELECT name FROM issue_state_defaults WHERE type = 'open'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(open_default, "open");

        // The default of a type is one row, so there is no second one to add.
        let second_default =
            sqlx::query("INSERT INTO issue_state_defaults (type, name) VALUES ('open', 'triage')")
                .execute(&pool)
                .await;
        assert!(
            second_default.is_err(),
            "a type must not carry two default states"
        );

        // Replacing it in place is the only way to move it, and it stays a
        // single write with no moment where the type has none.
        sqlx::query(
            "INSERT INTO issue_state_defaults (type, name) VALUES ('open', 'triage')
             ON CONFLICT(type) DO UPDATE SET name = excluded.name",
        )
        .execute(&pool)
        .await
        .unwrap();

        // A default must be a state of the type it is default for.
        let foreign_type = sqlx::query(
            "INSERT INTO issue_state_defaults (type, name) VALUES ('in progress', 'triage')
             ON CONFLICT(type) DO UPDATE SET name = excluded.name",
        )
        .execute(&pool)
        .await;
        assert!(
            foreign_type.is_err(),
            "a default must belong to the type it is default for"
        );

        // ...and must be a state that exists.
        let absent = sqlx::query(
            "INSERT INTO issue_state_defaults (type, name) VALUES ('open', 'ghost')
             ON CONFLICT(type) DO UPDATE SET name = excluded.name",
        )
        .execute(&pool)
        .await;
        assert!(absent.is_err(), "a default must name a state that exists");

        // A type that still has states cannot lose its default, whether the row
        // is removed directly or by deleting the state it names.
        for statement in [
            "DELETE FROM issue_state_defaults WHERE type = 'open'",
            "DELETE FROM issue_states WHERE name = 'triage'",
        ] {
            assert!(
                sqlx::query(statement).execute(&pool).await.is_err(),
                "a populated type must keep a default: {statement}"
            );
        }

        // Emptying a type is how its default legitimately goes away, which is
        // what lets the optional `in progress` type be empty.
        sqlx::query("DELETE FROM issue_states WHERE name = 'in progress'")
            .execute(&pool)
            .await
            .unwrap();
        let in_progress_defaults: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issue_state_defaults WHERE type = 'in progress'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(in_progress_defaults, 0);

        // Refilling it hands the default straight to the state that arrives,
        // by creation or by retyping one in.
        sqlx::query("UPDATE issue_states SET type = 'in progress' WHERE name = 'not planned'")
            .execute(&pool)
            .await
            .unwrap();
        let refilled: String =
            sqlx::query_scalar("SELECT name FROM issue_state_defaults WHERE type = 'in progress'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(refilled, "not planned");

        // `issues.state` is a real reference, so an issue in an unconfigured
        // state is unrepresentable rather than merely unexpected.
        sqlx::query("INSERT INTO repos (id, identity_key, name) VALUES (1, 'fk', 'fk')")
            .execute(&pool)
            .await
            .unwrap();
        let unconfigured = sqlx::query(
            "INSERT INTO issues (repo_id, number, title, state) VALUES (1, 1, 'Orphan', 'nowhere')",
        )
        .execute(&pool)
        .await;
        assert!(
            unconfigured.is_err(),
            "an issue must not reference a state that is not configured"
        );
        sqlx::query(
            "INSERT INTO issues (repo_id, number, title, state) VALUES (1, 1, 'Real', 'open')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Deleting an occupied state is refused rather than taking its issues
        // with it.
        let occupied = sqlx::query("DELETE FROM issue_states WHERE name = 'open'")
            .execute(&pool)
            .await;
        assert!(
            occupied.is_err(),
            "a state with issues in it must not be deletable"
        );
        let survivors: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM issues")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            survivors, 1,
            "the refused delete must not have removed issues"
        );

        // Renaming carries the issues instead, in one statement.
        sqlx::query("UPDATE issue_states SET name = 'ready' WHERE name = 'open'")
            .execute(&pool)
            .await
            .unwrap();
        let moved: String = sqlx::query_scalar("SELECT state FROM issues WHERE number = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            moved, "ready",
            "rename must carry the state's issues with it"
        );

        let issue_columns: Vec<String> = sqlx::query!("PRAGMA table_info('issues')")
            .fetch_all(&pool)
            .await
            .unwrap()
            .into_iter()
            .map(|row| row.name)
            .collect();
        assert!(!issue_columns.iter().any(|name| name == "priority"));
        assert!(!issue_columns.iter().any(|name| name == "lease_id"));
        assert!(!issue_columns.iter().any(|name| name == "acquired_at"));

        for table in [
            "issue_leases",
            "projects",
            "issue_projects",
            "issue_parents",
            "issue_relations",
            "issue_pr_links",
            "project_label_groups",
            "project_labels",
            "project_label_links",
            "project_milestones",
            "issue_milestones",
        ] {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS(
                       SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?
                   ) AS "exists!: bool""#,
                table
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(exists, "missing table {table}");
        }
    }

    #[tokio::test]
    async fn seeding_installs_the_default_workflow_once_and_preserves_customizations() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        let repo = crate::sql::repo::upsert(&pool, "/seed/.git", "seed")
            .await
            .unwrap();

        let initial: Vec<(String, String, bool)> = sqlx::query!(
            r#"SELECT s.name AS "name!: String", s.type AS "state_type!: String",
                      (d.name IS NOT NULL) AS "is_default!: bool"
               FROM issue_states s
               LEFT JOIN issue_state_defaults d ON d.name = s.name
               ORDER BY s.name"#
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.name, row.state_type, row.is_default))
        .collect();
        // Ordered by name: the table stores no ordering of its own.
        assert_eq!(
            initial,
            vec![
                ("closed".to_string(), "closed".to_string(), true),
                ("in progress".to_string(), "in progress".to_string(), true),
                ("not planned".to_string(), "closed".to_string(), false),
                ("open".to_string(), "open".to_string(), true),
            ]
        );
        for state_type in [StateType::Open, StateType::InProgress, StateType::Closed] {
            let default = crate::sql::issue::default_state(&pool, state_type)
                .await
                .unwrap();
            assert_eq!(default.as_deref(), Some(state_type.as_str()));
        }

        crate::app::issue::add_state(&pool, "Custom", StateType::Open, false)
            .await
            .unwrap();
        crate::app::issue::delete_state(&pool, "not planned", None)
            .await
            .unwrap();
        crate::app::issue::create(
            &pool,
            repo,
            "Preserved",
            "body",
            Some("Custom"),
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let states_before: Vec<String> = sqlx::query_scalar!(
            r#"SELECT json_array(s.name, s.type, d.name IS NOT NULL) AS "state!: String"
               FROM issue_states s
               LEFT JOIN issue_state_defaults d ON d.name = s.name
               ORDER BY s.name"#
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let issues_before: Vec<String> = sqlx::query_scalar!(
            r#"SELECT json_array(number, title, body, state) AS "issue!: String"
               FROM issues WHERE repo_id = ? ORDER BY number"#,
            repo
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        crate::sql::repo::upsert(&pool, "/seed/.git", "seed")
            .await
            .unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();

        let states_after: Vec<String> = sqlx::query_scalar!(
            r#"SELECT json_array(s.name, s.type, d.name IS NOT NULL) AS "state!: String"
               FROM issue_states s
               LEFT JOIN issue_state_defaults d ON d.name = s.name
               ORDER BY s.name"#
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let issues_after: Vec<String> = sqlx::query_scalar!(
            r#"SELECT json_array(number, title, body, state) AS "issue!: String"
               FROM issues WHERE repo_id = ? ORDER BY number"#,
            repo
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(states_after, states_before);
        assert_eq!(issues_after, issues_before);
    }

    #[tokio::test]
    async fn relation_and_pr_link_schema_enforce_repo_scope_and_atomicity() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        let first_repo = crate::sql::repo::upsert(&pool, "/first/.git", "first")
            .await
            .unwrap();
        let second_repo = crate::sql::repo::upsert(&pool, "/second/.git", "second")
            .await
            .unwrap();
        crate::app::issue::create(&pool, first_repo, "First", "", None, None, None, None)
            .await
            .unwrap();
        crate::app::issue::create(
            &pool,
            first_repo,
            "First repo second issue",
            "",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        crate::app::issue::create(&pool, second_repo, "Second", "", None, None, None, None)
            .await
            .unwrap();
        crate::app::pr::create(&pool, first_repo, "One", "", "one", None, None)
            .await
            .unwrap();
        crate::app::pr::create(&pool, first_repo, "Two", "", "two", None, None)
            .await
            .unwrap();

        let cross_repo_relation = sqlx::query!(
            "INSERT INTO issue_relations (repo_id, low_number, high_number) VALUES (?, 1, 2)",
            second_repo
        )
        .execute(&pool)
        .await;
        assert!(cross_repo_relation.is_err());

        let cross_repo_pr = sqlx::query!(
            "INSERT INTO issue_pr_links (repo_id, issue_number, pr_number) VALUES (?, 1, 2)",
            second_repo
        )
        .execute(&pool)
        .await;
        assert!(cross_repo_pr.is_err());

        let first_lease = crate::sql::issue::acquire_lease(&pool, first_repo, 1)
            .await
            .unwrap()
            .unwrap();
        let second_lease = crate::sql::issue::acquire_lease(&pool, first_repo, 2)
            .await
            .unwrap()
            .unwrap();
        for (issue, pr, lease) in [
            (1, 1, first_lease.as_str()),
            (1, 2, first_lease.as_str()),
            (2, 1, second_lease.as_str()),
        ] {
            crate::app::pr::link(&pool, first_repo, issue, pr, Some(lease))
                .await
                .unwrap();
        }
        let links: Vec<(i64, i64)> = sqlx::query!(
            r#"SELECT issue_number AS "issue_number!: i64", pr_number AS "pr_number!: i64"
               FROM issue_pr_links WHERE repo_id = ? ORDER BY issue_number, pr_number"#,
            first_repo
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.issue_number, row.pr_number))
        .collect();
        assert_eq!(links, vec![(1, 1), (1, 2), (2, 1)]);

        crate::app::pr::link(&pool, first_repo, 1, 1, Some(&first_lease))
            .await
            .unwrap();
        let duplicate_pair = sqlx::query!(
            "INSERT INTO issue_pr_links (repo_id, issue_number, pr_number) VALUES (?, 1, 1)",
            first_repo
        )
        .execute(&pool)
        .await;
        assert!(duplicate_pair.is_err());

        crate::app::pr::unlink(&pool, first_repo, 1, 1, Some(&first_lease))
            .await
            .unwrap();
        let remaining: Vec<(i64, i64)> = sqlx::query!(
            r#"SELECT issue_number AS "issue_number!: i64", pr_number AS "pr_number!: i64"
               FROM issue_pr_links WHERE repo_id = ? ORDER BY issue_number, pr_number"#,
            first_repo
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.issue_number, row.pr_number))
        .collect();
        assert_eq!(remaining, vec![(1, 2), (2, 1)]);

        let failed =
            crate::sql::pr::insert_linked(&pool, second_repo, "Orphan", "", "orphan", 999, None)
                .await;
        assert!(failed.is_err());
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM prs WHERE repo_id = ?", second_repo)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "failed linked create left an orphan PR");
    }
}
