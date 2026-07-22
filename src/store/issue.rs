//! Issue application facade. It resolves the active repository and delegates
//! persistence to `crate::sql::issue`; this module intentionally has no SQL.

use super::Store;
use crate::domain::{
    issue::{Issue, IssueDetail, IssueState, LockOutcome},
    StateFilter,
};
use anyhow::Result;

impl Store {
    pub async fn create_issue(&self, title: &str, body: &str) -> Result<i64> {
        crate::app::issue::create(&self.pool, self.repo_id()?, title, body).await
    }

    pub async fn list_issues(
        &self,
        filter: StateFilter,
        state_name: Option<&str>,
        label: Option<&str>,
        unblocked: bool,
    ) -> Result<Vec<Issue>> {
        let repo = (!self.is_all()).then(|| self.repo_id()).transpose()?;
        crate::app::issue::list(&self.pool, repo, filter, state_name, label, unblocked).await
    }

    pub async fn issue_detail(&self, number: i64) -> Result<IssueDetail> {
        crate::app::issue::detail(&self.pool, self.repo_id()?, number).await
    }

    pub async fn add_issue_comment(&self, number: i64, body: &str) -> Result<()> {
        crate::app::issue::comment(&self.pool, self.repo_id()?, number, body).await
    }

    pub async fn set_issue_state(&self, number: i64, state: &str) -> Result<()> {
        crate::app::issue::set_state(&self.pool, self.repo_id()?, number, state).await
    }

    pub async fn close_issue(&self, number: i64) -> Result<String> {
        crate::app::issue::close(&self.pool, self.repo_id()?, number).await
    }

    pub async fn reopen_issue(&self, number: i64) -> Result<String> {
        crate::app::issue::reopen(&self.pool, self.repo_id()?, number).await
    }

    pub async fn edit_issue(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::edit(&self.pool, self.repo_id()?, number, title, body).await
    }

    pub async fn add_dependency(&self, blocker: i64, blocked: i64) -> Result<()> {
        crate::app::issue::add_dependency(&self.pool, self.repo_id()?, blocker, blocked).await
    }

    pub async fn remove_dependency(&self, blocker: i64, blocked: i64) -> Result<()> {
        crate::app::issue::remove_dependency(&self.pool, self.repo_id()?, blocker, blocked).await
    }

    pub async fn lock_issue(&self, number: i64, holder: &str) -> Result<LockOutcome> {
        crate::app::issue::lock(&self.pool, self.repo_id()?, number, holder).await
    }

    pub async fn unlock_issue(&self, number: i64, holder: &str, force: bool) -> Result<bool> {
        crate::app::issue::unlock(&self.pool, self.repo_id()?, number, holder, force).await
    }

    pub async fn list_states(&self) -> Result<Vec<IssueState>> {
        crate::app::issue::list_states(&self.pool, self.repo_id()?).await
    }

    pub async fn add_state(&self, name: &str, starting: bool, terminal: bool) -> Result<()> {
        crate::app::issue::add_state(&self.pool, self.repo_id()?, name, starting, terminal).await
    }
}
