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
mod pr;
mod wiki;

pub use crate::domain::issue::{LockOutcome, StateFilter};
pub use crate::domain::Comment;

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

/// A slug is lowercase alphanumerics and dashes — a stable, link-friendly key.
pub fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !out.is_empty() {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}
