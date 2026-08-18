//! Issue application facade. It resolves the active repository and delegates
//! persistence to `crate::sql::issue`; this module intentionally has no SQL.

use super::Store;
use crate::domain::issue::{
    Issue, IssueDetail, IssueListSelector, IssueState, LeaseOutcome, StateType,
};
use anyhow::Result;

impl Store {
    pub async fn create_issue(
        &self,
        title: &str,
        body: &str,
        state: Option<&str>,
        project: Option<&str>,
        milestone: Option<&str>,
        parent: Option<i64>,
    ) -> Result<i64> {
        crate::app::issue::create(
            &self.pool,
            self.repo_id()?,
            title,
            body,
            state,
            project,
            milestone,
            parent,
        )
        .await
    }

    pub async fn list_issues(
        &self,
        selector: IssueListSelector,
        label: Option<&str>,
        project: Option<&str>,
        milestone: Option<&str>,
        related_to: Option<i64>,
        unblocked: bool,
    ) -> Result<Vec<Issue>> {
        let repo = (!self.is_all()).then(|| self.repo_id()).transpose()?;
        crate::app::issue::list(
            &self.pool,
            repo,
            crate::app::issue::ListQuery {
                selector,
                label,
                project,
                milestone,
                related_to,
                unblocked,
            },
        )
        .await
    }

    pub async fn issue_detail(&self, number: i64) -> Result<IssueDetail> {
        crate::app::issue::detail(&self.pool, self.repo_id()?, number).await
    }

    /// All issues for the repository-scoped, read-only browser.
    pub async fn list_all_issue_details(&self) -> Result<Vec<IssueDetail>> {
        let repo = self.repo_id()?;
        let issues = crate::app::issue::list(
            &self.pool,
            Some(repo),
            crate::app::issue::ListQuery {
                selector: IssueListSelector::All,
                label: None,
                project: None,
                milestone: None,
                related_to: None,
                unblocked: false,
            },
        )
        .await?;
        let mut details = Vec::with_capacity(issues.len());
        for issue in issues {
            details.push(crate::app::issue::detail(&self.pool, repo, issue.number).await?);
        }
        Ok(details)
    }

    pub async fn add_issue_comment(&self, number: i64, body: &str) -> Result<()> {
        crate::app::issue::comment(&self.pool, self.repo_id()?, number, body).await
    }

    pub async fn set_issue_project(
        &self,
        number: i64,
        project: &str,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::set_project(&self.pool, self.repo_id()?, number, project, lease).await
    }

    pub async fn clear_issue_project(&self, number: i64, lease: Option<&str>) -> Result<()> {
        crate::app::issue::clear_project(&self.pool, self.repo_id()?, number, lease).await
    }

    pub async fn set_issue_milestone(
        &self,
        number: i64,
        milestone: &str,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::set_milestone(&self.pool, self.repo_id()?, number, milestone, lease)
            .await
    }

    pub async fn clear_issue_milestone(&self, number: i64, lease: Option<&str>) -> Result<()> {
        crate::app::issue::clear_milestone(&self.pool, self.repo_id()?, number, lease).await
    }

    pub async fn set_issue_parent(
        &self,
        child: i64,
        parent: i64,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::set_parent(&self.pool, self.repo_id()?, child, parent, lease).await
    }

    pub async fn clear_issue_parent(&self, child: i64, lease: Option<&str>) -> Result<()> {
        crate::app::issue::clear_parent(&self.pool, self.repo_id()?, child, lease).await
    }

    pub async fn set_issue_state(
        &self,
        number: i64,
        state: &str,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::set_state(&self.pool, self.repo_id()?, number, state, lease).await
    }

    pub async fn edit_issue(
        &self,
        number: i64,
        title: Option<&str>,
        body: Option<&str>,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::edit(&self.pool, self.repo_id()?, number, title, body, lease).await
    }

    pub async fn add_dependency(
        &self,
        leased_issue: i64,
        blocker: i64,
        blocked: i64,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::add_dependency(
            &self.pool,
            self.repo_id()?,
            leased_issue,
            blocker,
            blocked,
            lease,
        )
        .await
    }

    pub async fn remove_dependency(
        &self,
        leased_issue: i64,
        blocker: i64,
        blocked: i64,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::remove_dependency(
            &self.pool,
            self.repo_id()?,
            leased_issue,
            blocker,
            blocked,
            lease,
        )
        .await
    }

    pub async fn add_issue_relation(&self, a: i64, b: i64, lease: Option<&str>) -> Result<()> {
        crate::app::issue::add_relation(&self.pool, self.repo_id()?, a, b, lease).await
    }

    pub async fn remove_issue_relation(&self, a: i64, b: i64, lease: Option<&str>) -> Result<()> {
        crate::app::issue::remove_relation(&self.pool, self.repo_id()?, a, b, lease).await
    }

    pub async fn lock_issue(&self, number: i64) -> Result<LeaseOutcome> {
        crate::app::issue::lock(&self.pool, self.repo_id()?, number).await
    }

    pub async fn unlock_issue(
        &self,
        number: i64,
        lease: Option<&str>,
        force: bool,
    ) -> Result<bool> {
        crate::app::issue::unlock(&self.pool, self.repo_id()?, number, lease, force).await
    }

    pub async fn list_states(&self) -> Result<Vec<IssueState>> {
        crate::app::issue::list_states(&self.pool).await
    }

    pub async fn add_state(
        &self,
        name: &str,
        state_type: StateType,
        default: bool,
    ) -> Result<bool> {
        crate::app::issue::add_state(&self.pool, name, state_type, default).await
    }

    pub async fn set_state_config(
        &self,
        name: &str,
        new_name: Option<&str>,
        state_type: Option<StateType>,
        default: bool,
    ) -> Result<bool> {
        crate::app::issue::set_state_config(&self.pool, name, new_name, state_type, default).await
    }

    pub async fn delete_state(&self, name: &str, move_to: Option<&str>) -> Result<i64> {
        crate::app::issue::delete_state(&self.pool, name, move_to).await
    }

    pub async fn start_issue(&self, number: i64, lease: Option<&str>) -> Result<String> {
        crate::app::issue::start(&self.pool, self.repo_id()?, number, lease).await
    }

    pub async fn close_issue(
        &self,
        number: i64,
        as_state: Option<&str>,
        lease: Option<&str>,
    ) -> Result<String> {
        crate::app::issue::close(&self.pool, self.repo_id()?, number, as_state, lease).await
    }

    pub async fn reopen_issue(
        &self,
        number: i64,
        as_state: Option<&str>,
        lease: Option<&str>,
    ) -> Result<String> {
        crate::app::issue::reopen(&self.pool, self.repo_id()?, number, as_state, lease).await
    }
}
