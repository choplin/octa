use super::Store;
use crate::domain::project::{ProjectDetail, ProjectOverview};
use anyhow::Result;

impl Store {
    pub async fn create_project(
        &self,
        name: &str,
        summary: &str,
        description: &str,
        state: &str,
        terminal: bool,
        priority: i64,
    ) -> Result<i64> {
        crate::app::project::create(
            &self.pool,
            self.repo_id()?,
            name,
            summary,
            description,
            state,
            terminal,
            priority,
        )
        .await
    }

    pub async fn list_projects(&self, active_only: bool) -> Result<Vec<ProjectOverview>> {
        let repo = (!self.is_all()).then(|| self.repo_id()).transpose()?;
        crate::app::project::list(&self.pool, repo, active_only).await
    }

    pub async fn project_detail(&self, reference: &str) -> Result<ProjectDetail> {
        crate::app::project::detail(&self.pool, self.repo_id()?, reference).await
    }

    pub async fn edit_project(
        &self,
        reference: &str,
        name: Option<&str>,
        summary: Option<&str>,
        description: Option<&str>,
        priority: Option<i64>,
    ) -> Result<()> {
        crate::app::project::edit(
            &self.pool,
            self.repo_id()?,
            reference,
            name,
            summary,
            description,
            priority,
        )
        .await
    }

    pub async fn set_project_state(
        &self,
        reference: &str,
        state: &str,
        terminal: bool,
    ) -> Result<()> {
        crate::app::project::set_state(&self.pool, self.repo_id()?, reference, state, terminal)
            .await
    }
}
