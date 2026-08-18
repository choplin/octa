use crate::domain::milestone::ProjectMilestone;
use anyhow::{bail, Result};
use serde::Serialize;
use std::fmt;

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
    /// The type of the configured state, read from `project_states` on every
    /// load. Whether the project is closed is this and nothing else.
    #[serde(rename = "state_type")]
    pub state_type: ProjectStateType,
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

#[derive(Debug, Serialize)]
pub struct ProjectState {
    pub name: String,
    #[serde(rename = "type")]
    pub state_type: ProjectStateType,
    pub is_default: bool,
}

/// The single axis classifying a configured Project state.
///
/// Two values, not the three `StateType` carries. A Project is an outcome that
/// is either still open or finished with; whether work is under way inside it
/// is already readable from its issue tally, so a third type would restate a
/// derived signal without constraining anything. `Planned` and `In Progress`
/// are both open-type states that differ by name. Variant order is the
/// lifecycle order every listing sorts by.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectStateType {
    Open,
    Closed,
}

impl ProjectStateType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
        }
    }

    /// Parse the wire form used by the CLI, the schema, and GraphQL alike.
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "open" => Ok(Self::Open),
            "closed" => Ok(Self::Closed),
            other => bail!("unknown project state type {other:?}; use open or closed"),
        }
    }
}

impl fmt::Display for ProjectStateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}
