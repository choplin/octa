use super::Store;
use crate::domain::project::{ProjectDetail, ProjectOverview, ProjectState, ProjectStateType};
use anyhow::Result;

impl Store {
    pub async fn create_project(
        &self,
        name: &str,
        summary: &str,
        description: &str,
        state: Option<&str>,
    ) -> Result<i64> {
        crate::app::project::create(
            &self.pool,
            self.repo_id()?,
            name,
            summary,
            description,
            state,
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
    ) -> Result<()> {
        crate::app::project::edit(
            &self.pool,
            self.repo_id()?,
            reference,
            name,
            summary,
            description,
        )
        .await
    }

    pub async fn set_project_state(&self, reference: &str, state: &str) -> Result<()> {
        crate::app::project::set_state(&self.pool, self.repo_id()?, reference, state).await
    }

    pub async fn close_project(&self, reference: &str, as_state: Option<&str>) -> Result<String> {
        crate::app::project::close(&self.pool, self.repo_id()?, reference, as_state).await
    }

    pub async fn reopen_project(&self, reference: &str, as_state: Option<&str>) -> Result<String> {
        crate::app::project::reopen(&self.pool, self.repo_id()?, reference, as_state).await
    }

    pub async fn list_project_states(&self) -> Result<Vec<ProjectState>> {
        crate::app::project::list_states(&self.pool).await
    }

    pub async fn add_project_state(
        &self,
        name: &str,
        state_type: ProjectStateType,
        default: bool,
    ) -> Result<bool> {
        crate::app::project::add_state(&self.pool, name, state_type, default).await
    }

    pub async fn set_project_state_config(
        &self,
        name: &str,
        new_name: Option<&str>,
        default: bool,
    ) -> Result<()> {
        crate::app::project::set_state_config(&self.pool, name, new_name, default).await
    }

    pub async fn delete_project_state(&self, name: &str, move_to: Option<&str>) -> Result<i64> {
        crate::app::project::delete_state(&self.pool, name, move_to).await
    }
}
