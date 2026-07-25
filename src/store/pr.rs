//! Pull-request application facade. It resolves the active repository and
//! delegates workflow policy to `crate::app::pr`; this module contains no SQL.

use super::Store;
use crate::domain::{
    pr::{Pr, PrDetail},
    StateFilter,
};
use anyhow::Result;

impl Store {
    pub async fn create_pr(
        &self,
        title: &str,
        body: &str,
        branch: &str,
        issue: Option<i64>,
    ) -> Result<i64> {
        crate::app::pr::create(&self.pool, self.repo_id()?, title, body, branch, issue).await
    }
    pub async fn list_prs(&self, filter: StateFilter) -> Result<Vec<Pr>> {
        crate::app::pr::list(
            &self.pool,
            (!self.is_all()).then(|| self.repo_id()).transpose()?,
            filter,
        )
        .await
    }
    pub async fn pr_detail(&self, number: i64) -> Result<PrDetail> {
        crate::app::pr::detail(&self.pool, self.repo_id()?, number).await
    }
    pub async fn add_pr_comment(&self, number: i64, body: &str) -> Result<()> {
        crate::app::pr::comment(&self.pool, self.repo_id()?, number, body).await
    }
    pub async fn set_pr_state(&self, number: i64, state: &str) -> Result<()> {
        crate::app::pr::set_state(&self.pool, self.repo_id()?, number, state).await
    }
    pub async fn edit_pr(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        crate::app::pr::edit(&self.pool, self.repo_id()?, number, title, body).await
    }

    pub async fn link_pr(&self, issue: i64, pr: i64) -> Result<()> {
        crate::app::pr::link(&self.pool, self.repo_id()?, issue, pr).await
    }

    pub async fn unlink_pr(&self, issue: i64, pr: i64) -> Result<()> {
        crate::app::pr::unlink(&self.pool, self.repo_id()?, issue, pr).await
    }
}
