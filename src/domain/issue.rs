use crate::domain::Comment;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Issue {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locked_by: Option<String>,
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
    pub comments: Vec<Comment>,
}

#[derive(Debug, Serialize)]
pub struct IssueState {
    pub name: String,
    pub is_starting: bool,
    pub is_terminal: bool,
    pub position: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
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

pub enum LockOutcome {
    Acquired,
    AlreadyHeld(String),
}
