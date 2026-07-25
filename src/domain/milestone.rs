use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct MilestoneRef {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectMilestone {
    #[serde(skip)]
    pub(crate) repo_id: i64,
    pub project_id: i64,
    pub id: i64,
    pub position: i64,
    pub name: String,
    pub description: String,
    pub status: String,
    pub start_date: Option<String>,
    pub target_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
