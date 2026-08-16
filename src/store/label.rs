use super::Store;
use crate::domain::label::{Label, LabelGroup};
use anyhow::Result;
impl Store {
    pub async fn create_label_group(&self, name: &str, selection: &str) -> Result<()> {
        crate::app::label::create_group(&self.pool, name, selection).await
    }
    pub async fn create_label(&self, name: &str, group: Option<&str>) -> Result<()> {
        crate::app::label::create(&self.pool, name, group).await
    }
    pub async fn list_labels(&self) -> Result<Vec<Label>> {
        crate::app::label::list(&self.pool).await
    }
    pub async fn list_label_groups(&self) -> Result<Vec<LabelGroup>> {
        crate::app::label::list_groups(&self.pool).await
    }
    pub async fn label_issue(&self, number: i64, label: &str, lease: Option<&str>) -> Result<()> {
        crate::app::label::attach(&self.pool, self.repo_id()?, number, label, lease).await
    }
    pub async fn unlabel_issue(&self, number: i64, label: &str, lease: Option<&str>) -> Result<()> {
        crate::app::label::detach(&self.pool, self.repo_id()?, number, label, lease).await
    }
    pub async fn create_project_label_group(&self, name: &str, selection: &str) -> Result<()> {
        crate::app::label::create_project_group(&self.pool, name, selection).await
    }
    pub async fn create_project_label(&self, name: &str, group: Option<&str>) -> Result<()> {
        crate::app::label::create_project_label(&self.pool, name, group).await
    }
    pub async fn list_project_labels(&self) -> Result<Vec<Label>> {
        crate::app::label::list_project_labels(&self.pool).await
    }
    pub async fn list_project_label_groups(&self) -> Result<Vec<LabelGroup>> {
        crate::app::label::list_project_groups(&self.pool).await
    }
    pub async fn label_project(&self, project: &str, label: &str) -> Result<()> {
        crate::app::label::attach_project(&self.pool, self.repo_id()?, project, label).await
    }
    pub async fn unlabel_project(&self, project: &str, label: &str) -> Result<()> {
        crate::app::label::detach_project(&self.pool, self.repo_id()?, project, label).await
    }
}
