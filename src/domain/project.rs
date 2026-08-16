use crate::domain::milestone::ProjectMilestone;
use serde::Serialize;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ProjectRef {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct Project {
    #[serde(skip)]
    pub(crate) repo_id: i64,
    pub repo: String,
    pub id: i64,
    pub name: String,
    pub summary: String,
    pub description: String,
    pub state: String,
    pub is_terminal: bool,
    pub priority: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct ProjectOverview {
    #[serde(flatten)]
    pub project: Project,
    pub tally: ProjectTally,
    pub milestones: Vec<ProjectMilestone>,
}

#[derive(Debug, Default, Serialize)]
pub struct ProjectTally {
    pub open: i64,
    pub closed: i64,
    pub total: i64,
}

#[derive(Debug, Serialize)]
pub struct ProjectDetail {
    #[serde(flatten)]
    pub project: Project,
    pub tally: ProjectTally,
    pub issue_numbers: Vec<i64>,
    pub milestones: Vec<ProjectMilestone>,
    pub labels: Vec<String>,
}
