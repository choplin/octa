//! Pull request primitive: a per-repo numbered discussion entity tied to a git
//! branch. octa stores the discussion/state; the code and diff live on the git
//! side, and the diff-anchored review experience is revia's domain (see
//! `docs/adr/0002-revia-integration.md`).

use super::{Comment, StateFilter, Store};
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Pr {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub branch: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PrDetail {
    #[serde(flatten)]
    pub pr: Pr,
    pub comments: Vec<Comment>,
}

impl Store {
    /// Create a PR bound to `branch`, assigning the next per-repo number.
    pub async fn create_pr(&self, title: &str, body: &str, branch: &str) -> Result<i64> {
        let repo = self.repo_id()?;
        let number = sqlx::query_scalar!(
            r#"INSERT INTO prs (repo_id, number, title, body, branch)
               VALUES (?, (SELECT COALESCE(MAX(number), 0) + 1 FROM prs WHERE repo_id = ?), ?, ?, ?)
               RETURNING number AS "number!: i64""#,
            repo,
            repo,
            title,
            body,
            branch
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(number)
    }

    pub async fn get_pr(&self, number: i64) -> Result<Option<Pr>> {
        let repo = self.repo_id()?;
        let pr = sqlx::query_as!(
            Pr,
            r#"SELECT r.name       AS "repo!: String",
                      p.number     AS "number!: i64",
                      p.title      AS "title!: String",
                      p.body       AS "body!: String",
                      p.branch     AS "branch!: String",
                      p.state      AS "state!: String",
                      p.created_at AS "created_at!: String",
                      p.updated_at AS "updated_at!: String"
               FROM prs p JOIN repos r ON r.id = p.repo_id
               WHERE p.repo_id = ? AND p.number = ?"#,
            repo,
            number
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(pr)
    }

    async fn require_pr(&self, number: i64) -> Result<Pr> {
        match self.get_pr(number).await? {
            Some(pr) => Ok(pr),
            None => bail!("PR #{number} not found"),
        }
    }

    /// List PRs. `open` = state 'open', `closed` = anything else, `all` = every.
    pub async fn list_prs(&self, filter: StateFilter) -> Result<Vec<Pr>> {
        if self.is_all() {
            let rows = sqlx::query_as!(
                Pr,
                r#"SELECT r.name AS "repo!: String", p.number AS "number!: i64",
                          p.title AS "title!: String", p.body AS "body!: String",
                          p.branch AS "branch!: String", p.state AS "state!: String",
                          p.created_at AS "created_at!: String", p.updated_at AS "updated_at!: String"
                   FROM prs p JOIN repos r ON r.id = p.repo_id
                   ORDER BY r.name, p.number"#
            )
            .fetch_all(&self.pool)
            .await?;
            return Ok(Self::filter_prs(rows, filter));
        }
        let repo = self.repo_id()?;
        let rows = sqlx::query_as!(
            Pr,
            r#"SELECT r.name AS "repo!: String", p.number AS "number!: i64",
                      p.title AS "title!: String", p.body AS "body!: String",
                      p.branch AS "branch!: String", p.state AS "state!: String",
                      p.created_at AS "created_at!: String", p.updated_at AS "updated_at!: String"
               FROM prs p JOIN repos r ON r.id = p.repo_id
               WHERE p.repo_id = ? ORDER BY p.number"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(Self::filter_prs(rows, filter))
    }

    fn filter_prs(rows: Vec<Pr>, filter: StateFilter) -> Vec<Pr> {
        rows.into_iter()
            .filter(|p| match filter {
                StateFilter::Open => p.state == "open",
                StateFilter::Closed => p.state != "open",
                StateFilter::All => true,
            })
            .collect()
    }

    pub async fn pr_detail(&self, number: i64) -> Result<PrDetail> {
        let repo = self.repo_id()?;
        let pr = self.require_pr(number).await?;
        let comments = sqlx::query_as!(
            Comment,
            r#"SELECT id AS "id!: i64", body AS "body!: String", created_at AS "created_at!: String"
               FROM pr_comments WHERE repo_id = ? AND pr_number = ? ORDER BY id"#,
            repo,
            number
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(PrDetail { pr, comments })
    }

    pub async fn add_pr_comment(&self, number: i64, body: &str) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_pr(number).await?;
        sqlx::query!(
            "INSERT INTO pr_comments (repo_id, pr_number, body) VALUES (?, ?, ?)",
            repo,
            number,
            body
        )
        .execute(&self.pool)
        .await?;
        self.touch_pr(repo, number).await
    }

    pub async fn set_pr_state(&self, number: i64, state: &str) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_pr(number).await?;
        sqlx::query!(
            "UPDATE prs SET state = ?, updated_at = datetime('now') \
             WHERE repo_id = ? AND number = ?",
            state,
            repo,
            number
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn edit_pr(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        let repo = self.repo_id()?;
        self.require_pr(number).await?;
        if title.is_none() && body.is_none() {
            bail!("nothing to update: pass --title and/or --body");
        }
        if let Some(t) = title {
            sqlx::query!(
                "UPDATE prs SET title = ? WHERE repo_id = ? AND number = ?",
                t,
                repo,
                number
            )
            .execute(&self.pool)
            .await?;
        }
        if let Some(b) = body {
            sqlx::query!(
                "UPDATE prs SET body = ? WHERE repo_id = ? AND number = ?",
                b,
                repo,
                number
            )
            .execute(&self.pool)
            .await?;
        }
        self.touch_pr(repo, number).await
    }

    async fn touch_pr(&self, repo: i64, number: i64) -> Result<()> {
        sqlx::query!(
            "UPDATE prs SET updated_at = datetime('now') WHERE repo_id = ? AND number = ?",
            repo,
            number
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
