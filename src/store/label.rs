//! Label primitive: repo-scoped labels, optionally grouped. A `single` group is
//! mutually exclusive (assigning one of its labels replaces any sibling on the
//! issue); a `multi` group and ungrouped labels coexist. octa provides the
//! mechanism only — the taxonomy and meaning are left to each project.

use super::Store;
use anyhow::{bail, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Label {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LabelGroup {
    pub name: String,
    /// "single" or "multi".
    pub selection: String,
}

impl Store {
    pub async fn create_label_group(&self, name: &str, selection: &str) -> Result<()> {
        let repo = self.repo_id()?;
        if selection != "single" && selection != "multi" {
            bail!("selection must be 'single' or 'multi'");
        }
        sqlx::query!(
            "INSERT INTO label_groups (repo_id, name, selection) VALUES (?, ?, ?)",
            repo,
            name,
            selection
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn create_label(&self, name: &str, group: Option<&str>) -> Result<()> {
        let repo = self.repo_id()?;
        if let Some(g) = group {
            let exists = sqlx::query_scalar!(
                "SELECT COUNT(*) FROM label_groups WHERE repo_id = ? AND name = ?",
                repo,
                g
            )
            .fetch_one(&self.pool)
            .await?;
            if exists == 0 {
                bail!("unknown label group {g:?}; create it first with `octa label group`");
            }
        }
        sqlx::query!(
            "INSERT INTO labels (repo_id, name, group_name) VALUES (?, ?, ?)",
            repo,
            name,
            group
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_labels(&self) -> Result<Vec<Label>> {
        let repo = self.repo_id()?;
        let rows = sqlx::query!(
            r#"SELECT name AS "name!: String", group_name AS "group_name?: String"
               FROM labels WHERE repo_id = ? ORDER BY group_name, name"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| Label {
                name: r.name,
                group: r.group_name,
            })
            .collect())
    }

    pub async fn list_label_groups(&self) -> Result<Vec<LabelGroup>> {
        let repo = self.repo_id()?;
        let rows = sqlx::query!(
            r#"SELECT name AS "name!: String", selection AS "selection!: String"
               FROM label_groups WHERE repo_id = ? ORDER BY name"#,
            repo
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|r| LabelGroup {
                name: r.name,
                selection: r.selection,
            })
            .collect())
    }

    /// Attach a label to an issue, enforcing single-select group exclusivity.
    pub async fn label_issue(&self, number: i64, label: &str) -> Result<()> {
        let repo = self.repo_id()?;
        self.issue_exists(repo, number).await?;
        let group = sqlx::query_scalar!(
            r#"SELECT group_name AS "g?: String" FROM labels WHERE repo_id = ? AND name = ?"#,
            repo,
            label
        )
        .fetch_optional(&self.pool)
        .await?;
        let group = match group {
            Some(g) => g,
            None => bail!("unknown label {label:?}; create it first with `octa label create`"),
        };

        // If the label belongs to a single-select group, drop any sibling first.
        if let Some(ref g) = group {
            let selection = sqlx::query_scalar!(
                r#"SELECT selection AS "s!: String" FROM label_groups WHERE repo_id = ? AND name = ?"#,
                repo,
                g
            )
            .fetch_one(&self.pool)
            .await?;
            if selection == "single" {
                sqlx::query!(
                    "DELETE FROM issue_labels
                     WHERE repo_id = ? AND issue_number = ? AND label_name IN (
                         SELECT name FROM labels WHERE repo_id = ? AND group_name = ?
                     )",
                    repo,
                    number,
                    repo,
                    g
                )
                .execute(&self.pool)
                .await?;
            }
        }

        sqlx::query!(
            "INSERT OR IGNORE INTO issue_labels (repo_id, issue_number, label_name) \
             VALUES (?, ?, ?)",
            repo,
            number,
            label
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn unlabel_issue(&self, number: i64, label: &str) -> Result<()> {
        let repo = self.repo_id()?;
        sqlx::query!(
            "DELETE FROM issue_labels WHERE repo_id = ? AND issue_number = ? AND label_name = ?",
            repo,
            number,
            label
        )
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn issue_exists(&self, repo: i64, number: i64) -> Result<()> {
        let exists = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM issues WHERE repo_id = ? AND number = ?",
            repo,
            number
        )
        .fetch_one(&self.pool)
        .await?;
        if exists == 0 {
            bail!("issue #{number} not found");
        }
        Ok(())
    }
}
