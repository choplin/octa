//! Issue application facade. It resolves the active repository and delegates
//! persistence to `crate::sql::issue`; this module intentionally has no SQL.

use super::Store;
use crate::domain::{
    issue::{Issue, IssueDetail, IssueState, LeaseOutcome},
    StateFilter,
};
use anyhow::Result;

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_issue(
        &self,
        title: &str,
        body: &str,
        state: Option<&str>,
        priority: i64,
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
            priority,
            project,
            milestone,
            parent,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn list_issues(
        &self,
        filter: StateFilter,
        state_name: Option<&str>,
        status_type: Option<&str>,
        priority: Option<i64>,
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
                filter,
                state_name,
                status_type,
                priority,
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
                filter: StateFilter::All,
                state_name: None,
                status_type: None,
                priority: None,
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
        priority: Option<i64>,
        lease: Option<&str>,
    ) -> Result<()> {
        crate::app::issue::edit(
            &self.pool,
            self.repo_id()?,
            number,
            title,
            body,
            priority,
            lease,
        )
        .await
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
        crate::app::issue::list_states(&self.pool, self.repo_id()?).await
    }

    pub async fn add_state(
        &self,
        name: &str,
        status_type: Option<&str>,
        starting: bool,
        terminal: bool,
    ) -> Result<()> {
        crate::app::issue::add_state(
            &self.pool,
            self.repo_id()?,
            name,
            status_type,
            starting,
            terminal,
        )
        .await
    }
}
