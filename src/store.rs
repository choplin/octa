//! Storage layer for octa: a single SQLite database shared across all worktrees.
//!
//! The database lives under the repository's common git directory (resolved via
//! `git rev-parse --git-common-dir`), so every linked worktree of the same
//! repository reads and writes the exact same issues.
//!
//! All SQL is kept out of this file: the schema lives in `migrations/` and every
//! query in `queries/`, checked against the schema at compile time by sqlx.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::path::PathBuf;
use std::process::Command;

/// Which issues `list` should return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
}

#[derive(Debug, Serialize)]
pub struct Issue {
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct Comment {
    pub id: i64,
    pub body: String,
    pub created_at: String,
}

/// An issue together with its comment thread, used for `show`.
#[derive(Debug, Serialize)]
pub struct IssueDetail {
    #[serde(flatten)]
    pub issue: Issue,
    pub comments: Vec<Comment>,
}

/// Resolve the path to the shared SQLite database under the common git dir.
pub fn resolve_db_path() -> Result<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--git-common-dir"])
        .output()
        .context("failed to run `git rev-parse --git-common-dir`")?;
    if !out.status.success() {
        bail!("not inside a git repository (git rev-parse --git-common-dir failed)");
    }
    let raw = String::from_utf8(out.stdout)
        .context("git printed non-UTF-8 output")?
        .trim()
        .to_string();

    let mut git_dir = PathBuf::from(&raw);
    if git_dir.is_relative() {
        git_dir = std::env::current_dir()?.join(git_dir);
    }
    // Canonicalize so the main worktree (which reports a relative ".git") and a
    // linked worktree (which reports an absolute path) resolve to the same file.
    let git_dir = std::fs::canonicalize(&git_dir)
        .with_context(|| format!("cannot resolve git dir {}", git_dir.display()))?;

    let octa_dir = git_dir.join("octa");
    std::fs::create_dir_all(&octa_dir)
        .with_context(|| format!("cannot create {}", octa_dir.display()))?;
    Ok(octa_dir.join("octa.db"))
}

pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Open (creating if needed) the shared database and apply migrations.
    pub async fn open() -> Result<Self> {
        let path = resolve_db_path()?;
        let options = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .with_context(|| format!("cannot open database at {}", path.display()))?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("cannot apply database migrations")?;
        Ok(Self { pool })
    }

    /// Create a new issue and return its auto-assigned number.
    pub async fn create(&self, title: &str, body: &str) -> Result<i64> {
        let result = sqlx::query_file!("queries/create_issue.sql", title, body)
            .execute(&self.pool)
            .await?;
        Ok(result.last_insert_rowid())
    }

    /// Fetch a single issue by number, if it exists.
    pub async fn get(&self, number: i64) -> Result<Option<Issue>> {
        let issue = sqlx::query_file_as!(Issue, "queries/get_issue.sql", number)
            .fetch_optional(&self.pool)
            .await?;
        Ok(issue)
    }

    async fn require(&self, number: i64) -> Result<Issue> {
        match self.get(number).await? {
            Some(issue) => Ok(issue),
            None => bail!("issue #{number} not found"),
        }
    }

    /// List issues filtered by state, ordered by number.
    pub async fn list(&self, filter: StateFilter) -> Result<Vec<Issue>> {
        let issues = match filter {
            StateFilter::All => {
                sqlx::query_file_as!(Issue, "queries/list_issues_all.sql")
                    .fetch_all(&self.pool)
                    .await?
            }
            StateFilter::Open => {
                sqlx::query_file_as!(Issue, "queries/list_issues_by_state.sql", "open")
                    .fetch_all(&self.pool)
                    .await?
            }
            StateFilter::Closed => {
                sqlx::query_file_as!(Issue, "queries/list_issues_by_state.sql", "closed")
                    .fetch_all(&self.pool)
                    .await?
            }
        };
        Ok(issues)
    }

    /// Append a comment to an existing issue and bump its updated_at.
    pub async fn add_comment(&self, number: i64, body: &str) -> Result<()> {
        self.require(number).await?;
        sqlx::query_file!("queries/insert_comment.sql", number, body)
            .execute(&self.pool)
            .await?;
        self.touch(number).await?;
        Ok(())
    }

    /// Fetch the comment thread of an issue in chronological order.
    pub async fn comments(&self, number: i64) -> Result<Vec<Comment>> {
        let comments = sqlx::query_file_as!(Comment, "queries/get_comments.sql", number)
            .fetch_all(&self.pool)
            .await?;
        Ok(comments)
    }

    /// Load an issue with its comments for `show`.
    pub async fn detail(&self, number: i64) -> Result<IssueDetail> {
        let issue = self.require(number).await?;
        let comments = self.comments(number).await?;
        Ok(IssueDetail { issue, comments })
    }

    /// Transition an issue to `open` or `closed`.
    pub async fn set_state(&self, number: i64, state: &str) -> Result<()> {
        self.require(number).await?;
        sqlx::query_file!("queries/set_state.sql", state, number)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Edit an issue's title and/or body. At least one must be provided.
    pub async fn edit(&self, number: i64, title: Option<&str>, body: Option<&str>) -> Result<()> {
        self.require(number).await?;
        if title.is_none() && body.is_none() {
            bail!("nothing to update: pass --title and/or --body");
        }
        if let Some(t) = title {
            sqlx::query_file!("queries/edit_title.sql", t, number)
                .execute(&self.pool)
                .await?;
        }
        if let Some(b) = body {
            sqlx::query_file!("queries/edit_body.sql", b, number)
                .execute(&self.pool)
                .await?;
        }
        self.touch(number).await?;
        Ok(())
    }

    async fn touch(&self, number: i64) -> Result<()> {
        sqlx::query_file!("queries/touch_issue.sql", number)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
