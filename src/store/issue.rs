//! Issue primitive: per-repo numbered discussion threads with configurable
//! states (starting/terminal flags), dependency edges (blocks / blocked-by),
//! an unblocked query, and an atomic exclusive lock.

use super::{Comment, Store};
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Issue {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct IssueDetail {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<String>,
    /// Issues that this issue blocks.
    pub blocks: Vec<i64>,
    /// Issues that block this issue.
    pub blocked_by: Vec<i64>,
    pub comments: Vec<Comment>,
}

#[derive(Debug, Serialize)]
pub struct IssueState {
    pub name: String,
    pub is_starting: bool,
    pub is_terminal: bool,
    pub position: i64,
}

/// Coarse state filter for `list`, orthogonal to an exact `--state <name>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    /// Non-terminal issues (the default "still open" view).
    Open,
    /// Terminal issues.
    Closed,
    /// Every issue.
    All,
}

/// Outcome of an atomic lock attempt.
pub enum LockOutcome {
    Acquired,
    AlreadyHeld(String),
}

impl Store {
    /// Create an issue in the active repo, assigning the next per-repo number
    /// atomically, and return that number.
    pub async fn create_issue(&self, title: &str, body: &str) -> Result<i64> {
        let repo = self.repo_id()?;
        let state = self.default_starting_state(repo).await?;
        let number = sqlx::query_scalar!(
            r#"INSERT INTO issues (repo_id, number, title, body, state)
               VALUES (?, (SELECT COALESCE(MAX(number), 0) + 1 FROM issues WHERE repo_id = ?), ?, ?, ?)
               RETURNING number AS "number!: i64""#,
            repo,
            repo,
            title,
            body,
            state
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(number)
    }

    async fn default_starting_state(&self, repo: i64) -> Result<String> {
        let name = sqlx::query_scalar!(
            "SELECT name FROM issue_states WHERE repo_id = ? AND is_starting = 1 \
             ORDER BY position LIMIT 1",
            repo
        )
        .fetch_optional(&self.pool)
        .await?;
        name.ok_or_else(|| anyhow::anyhow!("no starting state configured for this repo"))
    }

    async fn default_terminal_state(&self, repo: i64) -> Result<String> {
        let name = sqlx::query_scalar!(
            "SELECT name FROM issue_states WHERE repo_id = ? AND is_terminal = 1 \
             ORDER BY position LIMIT 1",
            repo
        )
        .fetch_optional(&self.pool)
        .await?;
        name.ok_or_else(|| anyhow::anyhow!("no terminal state configured for this repo"))
    }

    /// Fetch one issue in the active repo.
    pub async fn get_issue(&self, number: i64) -> Result<Option<Issue>> {
        let repo = self.repo_id()?;
        let issue = sqlx::query_as!(
            Issue,
            r#"SELECT r.name        AS "repo!: String",
                      i.number      AS "number!: i64",
                      i.title       AS "title!: String",
                      i.body        AS "body!: String",
                      i.state       AS "state!: String",
                      i.locked_by   AS "locked_by?: String",
                      i.created_at  AS "created_at!: String",
                      i.updated_at  AS "updated_at!: String"
               FROM issues i JOIN repos r ON r.id = i.repo_id
               WHERE i.repo_id = ? AND i.number = ?"#,
            repo,
            number
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(issue)
    }

    async fn require_issue(&self, number: i64) -> Result<Issue> {
        match self.get_issue(number).await? {
            Some(issue) => Ok(issue),
            None => bail!("issue #{number} not found"),
        }
    }

    /// List issues. Under `--all-repos` this is a simple cross-repo listing
    /// filtered only by `filter`; the per-repo refinements (`state_name`,
    /// `label`, `unblocked`) require a single repo.
    pub async fn list_issues(
        &self,
        filter: StateFilter,
        state_name: Option<&str>,
        label: Option<&str>,
        unblocked: bool,
    ) -> Result<Vec<Issue>> {
        if self.is_all() {
            if state_name.is_some() || label.is_some() || unblocked {
                bail!("--state <name>, --label and --unblocked need a single repository");
            }
            return self.list_issues_all_repos(filter).await;
        }
        let repo = self.repo_id()?;

        // Base rows with state flags.
        let rows = sqlx::query!(
            r#"SELECT r.name AS "repo!: String",
                      i.number AS "number!: i64",
                      i.title AS "title!: String",
                      i.body AS "body!: String",
                      i.state AS "state!: String",
                      i.locked_by AS "locked_by?: String",
                      i.created_at AS "created_at!: String",
                      i.updated_at AS "updated_at!: String",
                      COALESCE(s.is_terminal, 0) AS "is_terminal!: i64"
               FROM issues i
               JOIN repos r ON r.id = i.repo_id
               LEFT JOIN issue_states s ON s.repo_id = i.repo_id AND s.name = i.state
               WHERE i.repo_id = ?
               ORDER BY i.number"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;

        let unblocked_set = if unblocked {
            Some(self.unblocked_numbers(repo).await?)
        } else {
            None
        };
        let labelled: Option<std::collections::HashSet<i64>> = match label {
            Some(l) => Some(
                sqlx::query_scalar!(
                    r#"SELECT issue_number AS "n!: i64" FROM issue_labels
                       WHERE repo_id = ? AND label_name = ?"#,
                    repo,
                    l
                )
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .collect(),
            ),
            None => None,
        };

        let mut out = Vec::new();
        for row in rows {
            let terminal = row.is_terminal != 0;
            let keep = match filter {
                StateFilter::Open => !terminal,
                StateFilter::Closed => terminal,
                StateFilter::All => true,
            };
            if !keep {
                continue;
            }
            if let Some(name) = state_name {
                if row.state != name {
                    continue;
                }
            }
            if let Some(ref set) = labelled {
                if !set.contains(&row.number) {
                    continue;
                }
            }
            if let Some(ref set) = unblocked_set {
                if !set.contains(&row.number) {
                    continue;
                }
            }
            out.push(Issue {
                repo: row.repo,
                number: row.number,
                title: row.title,
                body: row.body,
                state: row.state,
                locked_by: row.locked_by,
                created_at: row.created_at,
                updated_at: row.updated_at,
            });
        }
        Ok(out)
    }

    async fn list_issues_all_repos(&self, filter: StateFilter) -> Result<Vec<Issue>> {
        let rows = sqlx::query!(
            r#"SELECT r.name AS "repo!: String",
                      i.number AS "number!: i64",
                      i.title AS "title!: String",
                      i.body AS "body!: String",
                      i.state AS "state!: String",
                      i.locked_by AS "locked_by?: String",
                      i.created_at AS "created_at!: String",
                      i.updated_at AS "updated_at!: String",
                      COALESCE(s.is_terminal, 0) AS "is_terminal!: i64"
               FROM issues i
               JOIN repos r ON r.id = i.repo_id
               LEFT JOIN issue_states s ON s.repo_id = i.repo_id AND s.name = i.state
               ORDER BY r.name, i.number"#
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::new();
        for row in rows {
            let terminal = row.is_terminal != 0;
            let keep = match filter {
                StateFilter::Open => !terminal,
                StateFilter::Closed => terminal,
                StateFilter::All => true,
            };
            if keep {
                out.push(Issue {
                    repo: row.repo,
                    number: row.number,
                    title: row.title,
                    body: row.body,
                    state: row.state,
                    locked_by: row.locked_by,
                    created_at: row.created_at,
                    updated_at: row.updated_at,
                });
            }
        }
        Ok(out)
    }

    /// Numbers of non-terminal issues whose every blocker is terminal.
    async fn unblocked_numbers(&self, repo: i64) -> Result<std::collections::HashSet<i64>> {
        let states = sqlx::query!(
            r#"SELECT i.number AS "number!: i64", COALESCE(s.is_terminal, 0) AS "is_terminal!: i64"
               FROM issues i
               LEFT JOIN issue_states s ON s.repo_id = i.repo_id AND s.name = i.state
               WHERE i.repo_id = ?"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        let terminal: std::collections::HashMap<i64, bool> = states
            .iter()
            .map(|r| (r.number, r.is_terminal != 0))
            .collect();

        let deps = sqlx::query!(
            r#"SELECT blocker_number AS "blocker!: i64", blocked_number AS "blocked!: i64"
               FROM issue_deps WHERE repo_id = ?"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        // blocked issues that still have an incomplete blocker.
        let mut blocked = std::collections::HashSet::new();
        for d in &deps {
            if !terminal.get(&d.blocker).copied().unwrap_or(false) {
                blocked.insert(d.blocked);
            }
        }

        Ok(terminal
            .iter()
            .filter(|(num, is_term)| !**is_term && !blocked.contains(num))
            .map(|(num, _)| *num)
            .collect())
    }

    /// Full issue view: labels, dependency edges, and comment thread.
    pub async fn issue_detail(&self, number: i64) -> Result<IssueDetail> {
        let repo = self.repo_id()?;
        let issue = self.require_issue(number).await?;
        let comments = sqlx::query_as!(
            Comment,
            r#"SELECT id AS "id!: i64", body AS "body!: String", created_at AS "created_at!: String"
               FROM comments WHERE repo_id = ? AND issue_number = ? ORDER BY id"#,
            repo,
            number
        )
        .fetch_all(&self.pool)
        .await?;
        let labels = sqlx::query_scalar!(
            r#"SELECT label_name AS "l!: String" FROM issue_labels
               WHERE repo_id = ? AND issue_number = ? ORDER BY label_name"#,
            repo,
            number
        )
        .fetch_all(&self.pool)
        .await?;
        let blocks = sqlx::query_scalar!(
            r#"SELECT blocked_number AS "n!: i64" FROM issue_deps
               WHERE repo_id = ? AND blocker_number = ? ORDER BY blocked_number"#,
            repo,
            number
        )
        .fetch_all(&self.pool)
        .await?;
        let blocked_by = sqlx::query_scalar!(
            r#"SELECT blocker_number AS "n!: i64" FROM issue_deps
               WHERE repo_id = ? AND blocked_number = ? ORDER BY blocker_number"#,
            repo,
            number
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(IssueDetail {
            issue,
            labels,
            blocks,
            blocked_by,
            comments,
        })
    }

    pub async fn add_issue_comment(&self, number: i64, body: &str) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_issue(number).await?;
        sqlx::query!(
            "INSERT INTO comments (repo_id, issue_number, body) VALUES (?, ?, ?)",
            repo,
            number,
            body
        )
        .execute(&self.pool)
        .await?;
        self.touch_issue(repo, number).await
    }

    /// Move an issue to an existing state.
    pub async fn set_issue_state(&self, number: i64, state: &str) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_issue(number).await?;
        let exists = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM issue_states WHERE repo_id = ? AND name = ?",
            repo,
            state
        )
        .fetch_one(&self.pool)
        .await?;
        if exists == 0 {
            bail!("unknown state {state:?}; add it first with `octa state add`");
        }
        sqlx::query!(
            "UPDATE issues SET state = ?, updated_at = datetime('now') \
             WHERE repo_id = ? AND number = ?",
            state,
            repo,
            number
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Move to the default terminal state (the `close` verb).
    pub async fn close_issue(&self, number: i64) -> Result<String> {
        let repo = self.repo_id()?;
        let state = self.default_terminal_state(repo).await?;
        self.set_issue_state(number, &state).await?;
        Ok(state)
    }

    /// Move to the default starting state (the `reopen` verb).
    pub async fn reopen_issue(&self, number: i64) -> Result<String> {
        let repo = self.repo_id()?;
        let state = self.default_starting_state(repo).await?;
        self.set_issue_state(number, &state).await?;
        Ok(state)
    }

    pub async fn edit_issue(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_issue(number).await?;
        if title.is_none() && body.is_none() {
            bail!("nothing to update: pass --title and/or --body");
        }
        if let Some(t) = title {
            sqlx::query!(
                "UPDATE issues SET title = ? WHERE repo_id = ? AND number = ?",
                t,
                repo,
                number
            )
            .execute(&self.pool)
            .await?;
        }
        if let Some(b) = body {
            sqlx::query!(
                "UPDATE issues SET body = ? WHERE repo_id = ? AND number = ?",
                b,
                repo,
                number
            )
            .execute(&self.pool)
            .await?;
        }
        self.touch_issue(repo, number).await
    }

    async fn touch_issue(&self, repo: i64, number: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE issues SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
            repo,
            number
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- Dependency edges ---------------------------------------------------

    /// Record that `blocker` blocks `blocked`.
    pub async fn add_dependency(&self, blocker: i64, blocked: i64) -> Result<()> {
        let repo = self.repo_id()?;
        if blocker == blocked {
            bail!("an issue cannot block itself");
        }
        self.require_issue(blocker).await?;
        self.require_issue(blocked).await?;
        sqlx::query!(
            "INSERT OR IGNORE INTO issue_deps (repo_id, blocker_number, blocked_number) \
             VALUES (?, ?, ?)",
            repo,
            blocker,
            blocked
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_dependency(&self, blocker: i64, blocked: i64) -> Result<()> {
        let repo = self.repo_id()?;
        sqlx::query!(
            "DELETE FROM issue_deps WHERE repo_id = ? AND blocker_number = ? AND blocked_number = ?",
            repo,
            blocker,
            blocked
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- Atomic lock --------------------------------------------------------

    /// Atomically acquire the exclusive lock on an issue for `holder`.
    pub async fn lock_issue(&self, number: i64, holder: &str) -> Result<LockOutcome> {
        let repo = self.repo_id()?;
        self.require_issue(number).await?;
        let affected = sqlx::query!(
            "UPDATE issues SET locked_by = ?, locked_at = datetime('now') \
             WHERE repo_id = ? AND number = ? AND locked_by IS NULL",
            holder,
            repo,
            number
        )
        .execute(&self.pool)
        .await?
        .rows_affected();
        if affected == 1 {
            return Ok(LockOutcome::Acquired);
        }
        let current = sqlx::query_scalar!(
            r#"SELECT locked_by AS "locked_by?: String" FROM issues WHERE repo_id = ? AND number = ?"#,
            repo,
            number
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(LockOutcome::AlreadyHeld(
            current.unwrap_or_else(|| "unknown".to_string()),
        ))
    }

    /// Release the lock. Only the holder may release it unless `force` is set.
    pub async fn unlock_issue(&self, number: i64, holder: &str, force: bool) -> Result<bool> {
        let repo = self.repo_id()?;
        self.require_issue(number).await?;
        let affected = if force {
            sqlx::query!(
                "UPDATE issues SET locked_by = NULL, locked_at = NULL \
                 WHERE repo_id = ? AND number = ?",
                repo,
                number
            )
            .execute(&self.pool)
            .await?
            .rows_affected()
        } else {
            sqlx::query!(
                "UPDATE issues SET locked_by = NULL, locked_at = NULL \
                 WHERE repo_id = ? AND number = ? AND locked_by = ?",
                repo,
                number,
                holder
            )
            .execute(&self.pool)
            .await?
            .rows_affected()
        };
        Ok(affected == 1)
    }

    // --- State configuration (mechanism only) -------------------------------

    pub async fn list_states(&self) -> Result<Vec<IssueState>> {
        let repo = self.repo_id()?;
        let rows = sqlx::query!(
            r#"SELECT name AS "name!: String",
                      is_starting AS "is_starting!: i64",
                      is_terminal AS "is_terminal!: i64",
                      position AS "position!: i64"
               FROM issue_states WHERE repo_id = ? ORDER BY position, name"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| IssueState {
                name: r.name,
                is_starting: r.is_starting != 0,
                is_terminal: r.is_terminal != 0,
                position: r.position,
            })
            .collect())
    }

    pub async fn add_state(&self, name: &str, starting: bool, terminal: bool) -> Result<()> {
        let repo = self.repo_id()?;
        let pos = sqlx::query_scalar!(
            r#"SELECT COALESCE(MAX(position), -1) + 1 AS "p!: i64"
               FROM issue_states WHERE repo_id = ?"#,
            repo
        )
        .fetch_one(&self.pool)
        .await?;
        let start = starting as i64;
        let term = terminal as i64;
        sqlx::query!(
            "INSERT INTO issue_states (repo_id, name, is_starting, is_terminal, position) \
             VALUES (?, ?, ?, ?, ?)",
            repo,
            name,
            start,
            term,
            pos
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
