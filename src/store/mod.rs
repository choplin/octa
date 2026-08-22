//! Storage layer for octa: a single global SQLite database under the XDG data
//! directory holds every entity for every repository. Entity rows carry a
//! `repository_id`, so the store is physically central but logically scoped per
//! repository — the current repository by default (resolved from cwd via the
//! git common directory), any named repository, or all of them at once.
//! Configuration — issue states, labels, and label groups — is global instead:
//! one set governs every repository.
//!
//! Repository identity is the repository root derived from the canonicalized git
//! common directory (works without a remote; every worktree of a repository shares
//! it). Concurrency rides on SQLite WAL plus a busy timeout; per-repository issue/pull request
//! numbers are assigned atomically by a single `INSERT ... RETURNING`, and
//! issue leases are acquired with a compare-and-set.

mod issue;
mod label;
mod milestone;
mod project;
mod pull_request;
mod repository;
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
pub enum RepositoryScope {
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

/// Resolve the path and human name of the repository at the cwd.
///
/// The path is the repository root derived from the canonicalized
/// `git rev-parse --git-common-dir`, shared by every worktree of the same
/// repository, and is what identifies it; the name is that root's directory
/// basename.
fn resolve_repository_identity() -> Result<(String, String)> {
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
    let root = repository_root(&git_dir);
    let path = root.to_string_lossy().to_string();
    let name = repository_name(&root);
    Ok((path, name))
}

/// Derive the repository root from the canonicalized git common dir.
///
/// A normal repository — including every linked worktree, whose common dir is
/// always the main repository's `.git` — ends in a `.git` component, and its
/// root is that component's parent. Any other layout (a bare repository such as
/// `/srv/repository.git`, or a `--separate-git-dir` store) has no `.git` component to
/// strip, so the common dir itself is the identity. Stripping unconditionally
/// there would collide every bare repository sharing a parent directory.
fn repository_root(git_dir: &std::path::Path) -> PathBuf {
    if git_dir.file_name().is_some_and(|n| n == ".git") {
        if let Some(parent) = git_dir.parent() {
            return parent.to_path_buf();
        }
    }
    git_dir.to_path_buf()
}

/// Derive a friendly repository name from the repository root's basename, falling
/// back to "repository".
fn repository_name(root: &std::path::Path) -> String {
    root.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repository".to_string())
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

/// Open the database only if it already exists, for reading.
///
/// `--help` reads the configured value sets so it can name them, and printing
/// help is not a reason to create a store or run migrations. A caller that
/// gets `None` has nothing to advertise, which is also the truth: a store that
/// does not exist holds no values.
pub async fn open_existing_pool() -> Option<SqlitePool> {
    let path = existing_db_path()?;
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(1));
    SqlitePoolOptions::new().connect_with(options).await.ok()
}

/// The database path, without creating anything along the way.
fn existing_db_path() -> Option<PathBuf> {
    let base = match std::env::var_os("XDG_DATA_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(std::env::var_os("HOME")?)
            .join(".local")
            .join("share"),
    };
    let path = base.join("octa").join("octa.db");
    path.is_file().then_some(path)
}

pub struct Store {
    pub(crate) pool: SqlitePool,
    scope: Resolved,
}

impl Store {
    /// Open the global database (creating it if needed), apply migrations, and
    /// resolve the requested scope.
    pub async fn open(scope: RepositoryScope) -> Result<Self> {
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
        // Configuration is global, so the default state sets are seeded once for
        // the store rather than once per repository.
        crate::sql::issue::seed_default_states(&pool).await?;
        crate::sql::project::seed_default_states(&pool).await?;

        let store = Self {
            pool,
            scope: Resolved::All,
        };
        let scope = match scope {
            RepositoryScope::Current => {
                let (path, name) = resolve_repository_identity()?;
                Resolved::One(store.upsert_repository(&path, &name).await?)
            }
            RepositoryScope::Named(name) => Resolved::One(store.repository_by_name(&name).await?),
            RepositoryScope::All => Resolved::All,
        };
        Ok(Self {
            pool: store.pool,
            scope,
        })
    }

    /// The active single repository, or an error when the scope is `--all-repositories`.
    pub(crate) fn repository_id(&self) -> Result<i64> {
        match self.scope {
            Resolved::One(id) => Ok(id),
            Resolved::All => {
                bail!("this command needs a single repository; drop --all-repositories")
            }
        }
    }

    pub(crate) fn is_all(&self) -> bool {
        matches!(self.scope, Resolved::All)
    }

    /// Insert the repository if new and return its id.
    async fn upsert_repository(&self, path: &str, name: &str) -> Result<i64> {
        crate::sql::repository::upsert(&self.pool, path, name).await
    }

    async fn repository_by_name(&self, name: &str) -> Result<i64> {
        crate::sql::repository::by_name(&self.pool, name).await
    }
}

#[cfg(test)]
mod repository_identity_tests {
    use super::{repository_name, repository_root};
    use std::path::Path;

    #[test]
    fn strips_the_dot_git_component_of_a_normal_repository() {
        let root = repository_root(Path::new("/home/dev/work/octa/.git"));
        assert_eq!(root, Path::new("/home/dev/work/octa"));
        assert_eq!(repository_name(&root), "octa");
    }

    /// Only the trailing `.git` is stripped; an ancestor directory that happens
    /// to be named `.git` stays part of the root.
    #[test]
    fn strips_only_the_trailing_component() {
        let root = repository_root(Path::new("/home/.git/checkouts/octa/.git"));
        assert_eq!(root, Path::new("/home/.git/checkouts/octa"));
    }

    #[test]
    fn keeps_a_path_that_does_not_end_in_dot_git() {
        let root = repository_root(Path::new("/srv/git/octa.git"));
        assert_eq!(root, Path::new("/srv/git/octa.git"));
        assert_eq!(repository_name(&root), "octa.git");
    }

    #[test]
    fn falls_back_to_repository_when_the_root_has_no_basename() {
        assert_eq!(repository_name(Path::new("/")), "repository");
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
                "repositories",
                rows!(
                    r#"SELECT json_array(id, path, name, created_at) AS "row!: String" FROM repositories ORDER BY id"#
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
                    r#"SELECT json_array(repository_id, number, title, body, state, created_at, updated_at) AS "row!: String" FROM issues ORDER BY repository_id, number"#
                ),
            ),
            (
                "issue_leases",
                rows!(
                    r#"SELECT json_array(repository_id, issue_number, lease_id, acquired_at) AS "row!: String" FROM issue_leases ORDER BY repository_id, issue_number"#
                ),
            ),
            (
                "issue_comments",
                rows!(
                    r#"SELECT json_array(id, repository_id, issue_number, body, created_at) AS "row!: String" FROM issue_comments ORDER BY id"#
                ),
            ),
            (
                "issue_dependencies",
                rows!(
                    r#"SELECT json_array(repository_id, blocker_number, blocked_number, created_at) AS "row!: String" FROM issue_dependencies ORDER BY repository_id, blocker_number, blocked_number"#
                ),
            ),
            (
                "pull_requests",
                rows!(
                    r#"SELECT json_array(repository_id, number, title, body, branch, state, created_at, updated_at) AS "row!: String" FROM pull_requests ORDER BY repository_id, number"#
                ),
            ),
            (
                "pull_request_comments",
                rows!(
                    r#"SELECT json_array(id, repository_id, pull_request_number, body, created_at) AS "row!: String" FROM pull_request_comments ORDER BY id"#
                ),
            ),
            (
                "wiki_pages",
                rows!(
                    r#"SELECT json_array(repository_id, slug, title, body, created_at, updated_at) AS "row!: String" FROM wiki_pages ORDER BY repository_id, slug"#
                ),
            ),
            (
                "wiki_links",
                rows!(
                    r#"SELECT json_array(repository_id, from_slug, to_slug) AS "row!: String" FROM wiki_links ORDER BY repository_id, from_slug, to_slug"#
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
                    r#"SELECT json_array(repository_id, issue_number, label_name) AS "row!: String" FROM issue_labels ORDER BY repository_id, issue_number, label_name"#
                ),
            ),
            (
                "project_states",
                rows!(
                    r#"SELECT json_array(name, type) AS "row!: String" FROM project_states ORDER BY name"#
                ),
            ),
            (
                "project_state_defaults",
                rows!(
                    r#"SELECT json_array(type, name) AS "row!: String" FROM project_state_defaults ORDER BY type"#
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
                    r#"SELECT json_array(repository_id, id, name, summary, description, state, created_at, updated_at) AS "row!: String" FROM projects ORDER BY repository_id, id"#
                ),
            ),
            (
                "project_label_links",
                rows!(
                    r#"SELECT json_array(repository_id, project_id, label_name) AS "row!: String" FROM project_label_links ORDER BY repository_id, project_id, label_name"#
                ),
            ),
            (
                "issue_projects",
                rows!(
                    r#"SELECT json_array(repository_id, issue_number, project_id) AS "row!: String" FROM issue_projects ORDER BY repository_id, issue_number"#
                ),
            ),
            (
                "issue_parents",
                rows!(
                    r#"SELECT json_array(repository_id, child_number, parent_number, created_at) AS "row!: String" FROM issue_parents ORDER BY repository_id, child_number"#
                ),
            ),
            (
                "issue_relations",
                rows!(
                    r#"SELECT json_array(repository_id, low_number, high_number, created_at) AS "row!: String" FROM issue_relations ORDER BY repository_id, low_number, high_number"#
                ),
            ),
            (
                "issue_pull_request_links",
                rows!(
                    r#"SELECT json_array(repository_id, issue_number, pull_request_number, created_at) AS "row!: String" FROM issue_pull_request_links ORDER BY repository_id, issue_number"#
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
        let repository = crate::sql::repository::upsert(&pool, "/test/.git", "test")
            .await
            .unwrap();
        let store = Store {
            pool,
            scope: Resolved::One(repository),
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

        // Store migrations, repository/state seeding, and fixture writes are complete
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
        crate::sql::repository::upsert(&store.pool, "/test/.git", "test")
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
            "seeding must name a default for every type it populates"
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

        // Refilling it is a create, never a retype: a state keeps the type it
        // was created with.
        let retyped =
            sqlx::query("UPDATE issue_states SET type = 'in progress' WHERE name = 'not planned'")
                .execute(&pool)
                .await;
        assert!(retyped.is_err(), "a state must not change type");
        sqlx::query("INSERT INTO issue_states (name, type) VALUES ('wip', 'in progress')")
            .execute(&pool)
            .await
            .unwrap();

        // The arriving state does not become the default on its own. Nothing in
        // the schema hands a default out, which is what lets a dump's own
        // defaults survive being replayed into a fresh database.
        let unclaimed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM issue_state_defaults WHERE type = 'in progress'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(unclaimed, 0, "the schema must not name a default by itself");
        sqlx::query("INSERT INTO issue_state_defaults (type, name) VALUES ('in progress', 'wip')")
            .execute(&pool)
            .await
            .unwrap();

        // `issues.state` is a real reference, so an issue in an unconfigured
        // state is unrepresentable rather than merely unexpected.
        sqlx::query("INSERT INTO repositories (id, path, name) VALUES (1, 'fk', 'fk')")
            .execute(&pool)
            .await
            .unwrap();
        let unconfigured = sqlx::query(
            "INSERT INTO issues (repository_id, number, title, state) VALUES (1, 1, 'Orphan', 'nowhere')",
        )
        .execute(&pool)
        .await;
        assert!(
            unconfigured.is_err(),
            "an issue must not reference a state that is not configured"
        );
        sqlx::query(
            "INSERT INTO issues (repository_id, number, title, state) VALUES (1, 1, 'Real', 'open')",
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
            "issue_pull_request_links",
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
        let repository = crate::sql::repository::upsert(&pool, "/seed/.git", "seed")
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
            repository,
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
               FROM issues WHERE repository_id = ? ORDER BY number"#,
            repository
        )
        .fetch_all(&pool)
        .await
        .unwrap();

        crate::sql::repository::upsert(&pool, "/seed/.git", "seed")
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
               FROM issues WHERE repository_id = ? ORDER BY number"#,
            repository
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(states_after, states_before);
        assert_eq!(issues_after, issues_before);
    }

    #[tokio::test]
    async fn relation_and_pull_request_link_schema_enforce_repository_scope_and_atomicity() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        crate::sql::issue::seed_default_states(&pool).await.unwrap();
        let first_repository = crate::sql::repository::upsert(&pool, "/first/.git", "first")
            .await
            .unwrap();
        let second_repository = crate::sql::repository::upsert(&pool, "/second/.git", "second")
            .await
            .unwrap();
        crate::app::issue::create(&pool, first_repository, "First", "", None, None, None, None)
            .await
            .unwrap();
        crate::app::issue::create(
            &pool,
            first_repository,
            "First repository second issue",
            "",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        crate::app::issue::create(
            &pool,
            second_repository,
            "Second",
            "",
            None,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        crate::app::pull_request::create(&pool, first_repository, "One", "", "one", None, None)
            .await
            .unwrap();
        crate::app::pull_request::create(&pool, first_repository, "Two", "", "two", None, None)
            .await
            .unwrap();

        let cross_repository_relation = sqlx::query!(
            "INSERT INTO issue_relations (repository_id, low_number, high_number) VALUES (?, 1, 2)",
            second_repository
        )
        .execute(&pool)
        .await;
        assert!(cross_repository_relation.is_err());

        let cross_repository_pull_request = sqlx::query!(
            "INSERT INTO issue_pull_request_links (repository_id, issue_number, pull_request_number) VALUES (?, 1, 2)",
            second_repository
        )
        .execute(&pool)
        .await;
        assert!(cross_repository_pull_request.is_err());

        let first_lease = crate::sql::issue::acquire_lease(&pool, first_repository, 1)
            .await
            .unwrap()
            .unwrap();
        let second_lease = crate::sql::issue::acquire_lease(&pool, first_repository, 2)
            .await
            .unwrap()
            .unwrap();
        for (issue, pull_request, lease) in [
            (1, 1, first_lease.as_str()),
            (1, 2, first_lease.as_str()),
            (2, 1, second_lease.as_str()),
        ] {
            crate::app::pull_request::link(
                &pool,
                first_repository,
                issue,
                pull_request,
                Some(lease),
            )
            .await
            .unwrap();
        }
        let links: Vec<(i64, i64)> = sqlx::query!(
            r#"SELECT issue_number AS "issue_number!: i64", pull_request_number AS "pull_request_number!: i64"
               FROM issue_pull_request_links WHERE repository_id = ? ORDER BY issue_number, pull_request_number"#,
            first_repository
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.issue_number, row.pull_request_number))
        .collect();
        assert_eq!(links, vec![(1, 1), (1, 2), (2, 1)]);

        crate::app::pull_request::link(&pool, first_repository, 1, 1, Some(&first_lease))
            .await
            .unwrap();
        let duplicate_pair = sqlx::query!(
            "INSERT INTO issue_pull_request_links (repository_id, issue_number, pull_request_number) VALUES (?, 1, 1)",
            first_repository
        )
        .execute(&pool)
        .await;
        assert!(duplicate_pair.is_err());

        crate::app::pull_request::unlink(&pool, first_repository, 1, 1, Some(&first_lease))
            .await
            .unwrap();
        let remaining: Vec<(i64, i64)> = sqlx::query!(
            r#"SELECT issue_number AS "issue_number!: i64", pull_request_number AS "pull_request_number!: i64"
               FROM issue_pull_request_links WHERE repository_id = ? ORDER BY issue_number, pull_request_number"#,
            first_repository
        )
        .fetch_all(&pool)
        .await
        .unwrap()
        .into_iter()
        .map(|row| (row.issue_number, row.pull_request_number))
        .collect();
        assert_eq!(remaining, vec![(1, 2), (2, 1)]);

        let failed = crate::sql::pull_request::insert_linked(
            &pool,
            second_repository,
            "Orphan",
            "",
            "orphan",
            999,
            None,
        )
        .await;
        assert!(failed.is_err());
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM pull_requests WHERE repository_id = ?",
            second_repository
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(count, 0, "failed linked create left an orphan pull request");
    }
}
