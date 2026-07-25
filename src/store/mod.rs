//! Storage layer for octa: a single global SQLite database under the XDG data
//! directory holds every entity for every repository. Each row carries a
//! `repo_id`, so the store is physically central but logically scoped per
//! repository — the current repository by default (resolved from cwd via the
//! git common directory), any named repository, or all of them at once.
//!
//! Repo identity is the canonicalized git common directory path (works without
//! a remote; every worktree of a repo shares it). Concurrency rides on SQLite
//! WAL plus a busy timeout; per-repo issue/PR numbers are assigned atomically by
//! a single `INSERT ... RETURNING`, and the issue lock is a compare-and-set.

mod issue;
mod label;
mod milestone;
mod pr;
mod project;
mod wiki;

pub use crate::domain::{issue::LockOutcome, StateFilter};

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

    /// Insert the repo if new, seed its default state set, and return its id.
    async fn upsert_repo(&self, identity_key: &str, name: &str) -> Result<i64> {
        crate::sql::repo::upsert(&self.pool, identity_key, name).await
    }

    async fn repo_by_name(&self, name: &str) -> Result<i64> {
        crate::sql::repo::by_name(&self.pool, name).await
    }
}

#[cfg(test)]
mod migration_tests {
    use super::{Resolved, Store};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    #[derive(Debug, Eq, PartialEq)]
    struct LogicalSnapshot(Vec<(&'static str, Vec<String>)>);

    async fn logical_snapshot(pool: &SqlitePool) -> LogicalSnapshot {
        async fn rows(pool: &SqlitePool, query: &str) -> Vec<String> {
            sqlx::query_scalar::<_, String>(query)
                .fetch_all(pool)
                .await
                .unwrap()
        }

        LogicalSnapshot(vec![
            (
                "repos",
                rows(pool, "SELECT json_array(id, identity_key, name, created_at) FROM repos ORDER BY id").await,
            ),
            (
                "issue_states",
                rows(pool, "SELECT json_array(repo_id, name, status_type, is_starting, is_terminal, position) FROM issue_states ORDER BY repo_id, position, name").await,
            ),
            (
                "issues",
                rows(pool, "SELECT json_array(repo_id, number, title, body, state, priority, locked_by, locked_at, created_at, updated_at) FROM issues ORDER BY repo_id, number").await,
            ),
            (
                "comments",
                rows(pool, "SELECT json_array(id, repo_id, issue_number, body, created_at) FROM comments ORDER BY id").await,
            ),
            (
                "issue_deps",
                rows(pool, "SELECT json_array(repo_id, blocker_number, blocked_number, created_at) FROM issue_deps ORDER BY repo_id, blocker_number, blocked_number").await,
            ),
            (
                "prs",
                rows(pool, "SELECT json_array(repo_id, number, title, body, branch, state, created_at, updated_at) FROM prs ORDER BY repo_id, number").await,
            ),
            (
                "pr_comments",
                rows(pool, "SELECT json_array(id, repo_id, pr_number, body, created_at) FROM pr_comments ORDER BY id").await,
            ),
            (
                "wiki_pages",
                rows(pool, "SELECT json_array(repo_id, slug, title, body, created_at, updated_at) FROM wiki_pages ORDER BY repo_id, slug").await,
            ),
            (
                "wiki_links",
                rows(pool, "SELECT json_array(repo_id, from_slug, to_slug) FROM wiki_links ORDER BY repo_id, from_slug, to_slug").await,
            ),
            (
                "label_groups",
                rows(pool, "SELECT json_array(repo_id, name, selection) FROM label_groups ORDER BY repo_id, name").await,
            ),
            (
                "labels",
                rows(pool, "SELECT json_array(repo_id, name, group_name) FROM labels ORDER BY repo_id, name").await,
            ),
            (
                "issue_labels",
                rows(pool, "SELECT json_array(repo_id, issue_number, label_name) FROM issue_labels ORDER BY repo_id, issue_number, label_name").await,
            ),
            (
                "projects",
                rows(pool, "SELECT json_array(repo_id, id, name, summary, description, state, status_type, priority, created_at, updated_at) FROM projects ORDER BY repo_id, id").await,
            ),
            (
                "issue_projects",
                rows(pool, "SELECT json_array(repo_id, issue_number, project_id) FROM issue_projects ORDER BY repo_id, issue_number").await,
            ),
            (
                "issue_parents",
                rows(pool, "SELECT json_array(repo_id, child_number, parent_number, created_at) FROM issue_parents ORDER BY repo_id, child_number").await,
            ),
            (
                "issue_relations",
                rows(pool, "SELECT json_array(repo_id, low_number, high_number, created_at) FROM issue_relations ORDER BY repo_id, low_number, high_number").await,
            ),
            (
                "issue_pr_links",
                rows(pool, "SELECT json_array(repo_id, issue_number, pr_number, created_at) FROM issue_pr_links ORDER BY repo_id, issue_number").await,
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
        let repo = crate::sql::repo::upsert(&pool, "/test/.git", "test")
            .await
            .unwrap();
        let store = Store {
            pool,
            scope: Resolved::One(repo),
        };
        for (name, status_type) in [
            ("Todo", "unstarted"),
            ("In Progress", "started"),
            ("In Review", "started"),
            ("Done", "completed"),
            ("Canceled", "canceled"),
        ] {
            store
                .add_state(name, Some(status_type), false, false)
                .await
                .unwrap();
        }
        let first = store
            .create_issue("First", "first body", Some("Todo"), 2, None, None, None)
            .await
            .unwrap();
        let second = store
            .create_issue(
                "Second",
                "second body",
                Some("In Progress"),
                1,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let review = store
            .create_issue(
                "Review",
                "review body",
                Some("In Review"),
                2,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let done = store
            .create_issue("Done", "done body", Some("Done"), 3, None, None, None)
            .await
            .unwrap();
        let canceled = store
            .create_issue(
                "Canceled",
                "canceled body",
                Some("Canceled"),
                4,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        let legacy_closed = store
            .create_issue(
                "Legacy closed",
                "legacy body",
                Some("closed"),
                0,
                None,
                None,
                None,
            )
            .await
            .unwrap();
        store
            .add_issue_comment(second, "still read-only")
            .await
            .unwrap();
        store.add_dependency(first, second).await.unwrap();

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

        let state_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('issue_states') ORDER BY cid")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(
            state_columns,
            [
                "repo_id",
                "name",
                "status_type",
                "is_starting",
                "is_terminal",
                "position"
            ]
        );

        let issue_columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('issues') ORDER BY cid")
                .fetch_all(&pool)
                .await
                .unwrap();
        assert!(issue_columns.iter().any(|name| name == "priority"));

        for table in [
            "projects",
            "issue_projects",
            "issue_parents",
            "issue_relations",
            "issue_pr_links",
            "project_milestones",
            "issue_milestones",
        ] {
            let exists: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type = 'table' AND name = ?)",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(exists, "missing table {table}");
        }
    }

    #[tokio::test]
    async fn repo_upsert_seeds_only_legacy_states_and_preserves_custom_workflow() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        let repo = crate::sql::repo::upsert(&pool, "/seed/.git", "seed")
            .await
            .unwrap();

        let initial: Vec<(String, String)> = sqlx::query_as(
            "SELECT name, status_type FROM issue_states WHERE repo_id = ? ORDER BY position, name",
        )
        .bind(repo)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            initial,
            vec![
                ("open".into(), "unstarted".into()),
                ("in_progress".into(), "started".into()),
                ("closed".into(), "completed".into()),
            ]
        );

        for (name, status_type) in [
            ("Backlog", "backlog"),
            ("In Review", "started"),
            ("Canceled", "canceled"),
            ("Custom", "unstarted"),
        ] {
            crate::app::issue::add_state(&pool, repo, name, Some(status_type), false, false)
                .await
                .unwrap();
        }
        crate::app::issue::create(
            &pool,
            repo,
            "Preserved",
            "body",
            Some("In Review"),
            2,
            None,
            None,
            None,
        )
        .await
        .unwrap();

        let states_before: Vec<String> = sqlx::query_scalar(
            "SELECT json_array(name, status_type, is_starting, is_terminal, position) FROM issue_states WHERE repo_id = ? ORDER BY position, name",
        )
        .bind(repo)
        .fetch_all(&pool)
        .await
        .unwrap();
        let issues_before: Vec<String> = sqlx::query_scalar(
            "SELECT json_array(number, title, body, state, priority) FROM issues WHERE repo_id = ? ORDER BY number",
        )
        .bind(repo)
        .fetch_all(&pool)
        .await
        .unwrap();

        crate::sql::repo::upsert(&pool, "/seed/.git", "seed")
            .await
            .unwrap();

        let states_after: Vec<String> = sqlx::query_scalar(
            "SELECT json_array(name, status_type, is_starting, is_terminal, position) FROM issue_states WHERE repo_id = ? ORDER BY position, name",
        )
        .bind(repo)
        .fetch_all(&pool)
        .await
        .unwrap();
        let issues_after: Vec<String> = sqlx::query_scalar(
            "SELECT json_array(number, title, body, state, priority) FROM issues WHERE repo_id = ? ORDER BY number",
        )
        .bind(repo)
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
        let first_repo = crate::sql::repo::upsert(&pool, "/first/.git", "first")
            .await
            .unwrap();
        let second_repo = crate::sql::repo::upsert(&pool, "/second/.git", "second")
            .await
            .unwrap();
        crate::app::issue::create(&pool, first_repo, "First", "", None, 0, None, None, None)
            .await
            .unwrap();
        crate::app::issue::create(
            &pool,
            first_repo,
            "First repo second issue",
            "",
            None,
            0,
            None,
            None,
            None,
        )
        .await
        .unwrap();
        crate::app::issue::create(&pool, second_repo, "Second", "", None, 0, None, None, None)
            .await
            .unwrap();
        crate::app::pr::create(&pool, first_repo, "One", "", "one", None)
            .await
            .unwrap();
        crate::app::pr::create(&pool, first_repo, "Two", "", "two", None)
            .await
            .unwrap();

        let cross_repo_relation = sqlx::query(
            "INSERT INTO issue_relations (repo_id, low_number, high_number) VALUES (?, 1, 2)",
        )
        .bind(second_repo)
        .execute(&pool)
        .await;
        assert!(cross_repo_relation.is_err());

        let cross_repo_pr = sqlx::query(
            "INSERT INTO issue_pr_links (repo_id, issue_number, pr_number) VALUES (?, 1, 2)",
        )
        .bind(second_repo)
        .execute(&pool)
        .await;
        assert!(cross_repo_pr.is_err());

        for (issue, pr) in [(1, 1), (1, 2), (2, 1)] {
            crate::app::pr::link(&pool, first_repo, issue, pr)
                .await
                .unwrap();
        }
        let links: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT issue_number, pr_number FROM issue_pr_links WHERE repo_id = ? ORDER BY issue_number, pr_number",
        )
        .bind(first_repo)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(links, vec![(1, 1), (1, 2), (2, 1)]);

        crate::app::pr::link(&pool, first_repo, 1, 1).await.unwrap();
        let duplicate_pair = sqlx::query(
            "INSERT INTO issue_pr_links (repo_id, issue_number, pr_number) VALUES (?, 1, 1)",
        )
        .bind(first_repo)
        .execute(&pool)
        .await;
        assert!(duplicate_pair.is_err());

        crate::app::pr::unlink(&pool, first_repo, 1, 1)
            .await
            .unwrap();
        let remaining: Vec<(i64, i64)> = sqlx::query_as(
            "SELECT issue_number, pr_number FROM issue_pr_links WHERE repo_id = ? ORDER BY issue_number, pr_number",
        )
        .bind(first_repo)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(remaining, vec![(1, 2), (2, 1)]);

        let failed =
            crate::sql::pr::insert_linked(&pool, second_repo, "Orphan", "", "orphan", 999).await;
        assert!(failed.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM prs WHERE repo_id = ?")
            .bind(second_repo)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "failed linked create left an orphan PR");
    }
}
