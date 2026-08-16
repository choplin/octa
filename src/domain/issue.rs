use crate::domain::{
    milestone::MilestoneRef, pr::PrRef, project::ProjectRef, state_filter::StateFilter, Comment,
};
use serde::Serialize;

pub fn validate_priority(priority: i64) -> anyhow::Result<i64> {
    if (0..=4).contains(&priority) {
        Ok(priority)
    } else {
        anyhow::bail!("priority must be between 0 and 4")
    }
}

#[derive(Debug, Serialize)]
pub struct Issue {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub priority: i64,
    pub project: Option<ProjectRef>,
    pub milestone: Option<MilestoneRef>,
    pub leased: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// An internal domain projection used while applying state filters. SQL maps
/// directly into this type; SQL row records never escape the persistence layer.
pub(crate) struct IssueListEntry {
    pub issue: Issue,
    pub is_terminal: bool,
}

#[derive(Debug, Serialize)]
pub struct IssueDetail {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<String>,
    pub blocks: Vec<i64>,
    pub blocked_by: Vec<i64>,
    pub related: Vec<i64>,
    pub pull_requests: Vec<PrRef>,
    pub parent: Option<IssueRef>,
    pub sub_issues: Vec<IssueRef>,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct IssueRef {
    pub number: i64,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct IssueState {
    pub name: String,
    pub is_starting: bool,
    pub is_terminal: bool,
}

impl StateFilter {
    pub fn includes(self, is_terminal: bool) -> bool {
        match self {
            Self::Open => !is_terminal,
            Self::Closed => is_terminal,
            Self::All => true,
        }
    }
}

pub enum LeaseOutcome {
    Acquired(String),
    AlreadyLeased,
}
