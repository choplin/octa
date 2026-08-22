//! Pull-request application facade. It resolves the active repository and
//! delegates workflow policy to `crate::app::pull_request`; this module contains no SQL.

use super::Store;
use crate::domain::{
    pull_request::{PullRequest, PullRequestDetail},
    StateFilter,
};
use anyhow::Result;

impl Store {
    pub async fn create_pull_request(
        &self,
        title: &str,
        body: &str,
        branch: &str,
        issue: Option<i64>,
        lease: Option<&str>,
    ) -> Result<i64> {
        crate::app::pull_request::create(
            &self.pool,
            self.repository_id()?,
            title,
            body,
            branch,
            issue,
            lease,
        )
        .await
    }
    pub async fn list_pull_requests(&self, filter: StateFilter) -> Result<Vec<PullRequest>> {
        crate::app::pull_request::list(
            &self.pool,
            (!self.is_all()).then(|| self.repository_id()).transpose()?,
            filter,
        )
        .await
    }
    pub async fn pull_request_detail(&self, number: i64) -> Result<PullRequestDetail> {
        crate::app::pull_request::detail(&self.pool, self.repository_id()?, number).await
    }
    pub async fn add_pull_request_comment(&self, number: i64, body: &str) -> Result<()> {
        crate::app::pull_request::comment(&self.pool, self.repository_id()?, number, body).await
    }
    pub async fn set_pull_request_state(&self, number: i64, state: &str) -> Result<()> {
        crate::app::pull_request::set_state(&self.pool, self.repository_id()?, number, state).await
    }
    pub async fn edit_pull_request(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        crate::app::pull_request::edit(&self.pool, self.repository_id()?, number, title, body).await
    }

    pub async fn link_pull_request(
        &self,
        issue: i64,
        pull_request: i64,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::pull_request::link(
            &self.pool,
            self.repository_id()?,
            issue,
            pull_request,
            lease,
        )
        .await
    }

    pub async fn unlink_pull_request(
        &self,
        issue: i64,
        pull_request: i64,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::pull_request::unlink(
            &self.pool,
            self.repository_id()?,
            issue,
            pull_request,
            lease,
        )
        .await
    }
}
