use super::Store;
use crate::domain::milestone::ProjectMilestone;
use anyhow::Result;

impl Store {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_project_milestone(
        &self,
        project: &str,
        name: &str,
        description: &str,
        status: &str,
        position: Option<i64>,
        start_date: Option<&str>,
        target_date: Option<&str>,
    ) -> Result<i64> {
        crate::app::milestone::create(
            &self.pool,
            self.repository_id()?,
            project,
            name,
            description,
            status,
            position,
            start_date,
            target_date,
        )
        .await
    }

    pub async fn list_project_milestones(&self, project: &str) -> Result<Vec<ProjectMilestone>> {
        crate::app::milestone::list(&self.pool, self.repository_id()?, project).await
    }

    pub async fn project_milestone(
        &self,
        project: &str,
        milestone: &str,
    ) -> Result<ProjectMilestone> {
        crate::app::milestone::show(&self.pool, self.repository_id()?, project, milestone).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn edit_project_milestone(
        &self,
        project: &str,
        milestone: &str,
        name: Option<&str>,
        description: Option<&str>,
        status: Option<&str>,
        position: Option<i64>,
        start_date: Option<&str>,
        target_date: Option<&str>,
        clear_start_date: bool,
        clear_target_date: bool,
    ) -> Result<()> {
        crate::app::milestone::edit(
            &self.pool,
            self.repository_id()?,
            project,
            milestone,
            name,
            description,
            status,
            position,
            start_date,
            target_date,
            clear_start_date,
            clear_target_date,
        )
        .await
    }
}
