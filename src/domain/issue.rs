use crate::domain::{
    milestone::MilestoneRef, pr::PrRef, project::ProjectRef, state_filter::StateFilter, Comment,
};
use serde::Serialize;
use std::fmt;
use std::str::FromStr;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StatusType {
    Backlog,
    Unstarted,
    Started,
    Completed,
    Canceled,
}

impl StatusType {
    pub const VALUES: &'static str = "backlog, unstarted, started, completed, canceled";

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Canceled)
    }
}

impl fmt::Display for StatusType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Backlog => "backlog",
            Self::Unstarted => "unstarted",
            Self::Started => "started",
            Self::Completed => "completed",
            Self::Canceled => "canceled",
        })
    }
}

impl FromStr for StatusType {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "backlog" => Ok(Self::Backlog),
            "unstarted" => Ok(Self::Unstarted),
            "started" => Ok(Self::Started),
            "completed" => Ok(Self::Completed),
            "canceled" => Ok(Self::Canceled),
            _ => anyhow::bail!(
                "unknown status type {value:?}; expected one of {}",
                Self::VALUES
            ),
        }
    }
}

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
    pub status_type: String,
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
    pub state_position: i64,
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
    pub status_type: String,
    pub is_starting: bool,
    pub is_terminal: bool,
    pub position: i64,
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
