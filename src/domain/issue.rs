use crate::domain::{
    milestone::MilestoneRef, project::ProjectRef, pull_request::PullRequestRef, Comment,
};
use anyhow::{anyhow, Result};
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
pub struct Issue {
    #[serde(rename = "repo")]
    pub repository: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub state: String,
    pub project: Option<ProjectRef>,
    pub milestone: Option<MilestoneRef>,
    pub leased: bool,
    pub created_at: String,
    pub updated_at: String,
}

/// An internal domain projection for list results. SQL row records never
/// escape the persistence layer.
pub(crate) struct IssueListEntry {
    pub issue: Issue,
    pub labels: Vec<String>,
    pub state_type: StateType,
}

/// The JSON projection returned by `issue list`.
///
/// List-only collections belong here rather than on `Issue`, which is also the
/// base of the heavier detail representation.
#[derive(Debug, Serialize)]
pub struct IssueListItem {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct IssueDetail {
    #[serde(flatten)]
    pub issue: Issue,
    pub labels: Vec<String>,
    pub blocks: Vec<i64>,
    pub blocked_by: Vec<i64>,
    pub related: Vec<i64>,
    pub pull_requests: Vec<PullRequestRef>,
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
    #[serde(rename = "type")]
    pub state_type: StateType,
    pub is_default: bool,
}

/// The single axis classifying a configured state.
///
/// The three values are the distinctions a user already draws before meeting
/// octa: not yet resolved, picked up, resolved. Why an issue closed is a
/// *reason*, carried by the state name rather than by a fourth type. Variant
/// order is the lifecycle order every listing sorts by.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StateType {
    Open,
    #[serde(rename = "in progress")]
    InProgress,
    Closed,
}

impl StateType {
    /// Every value, in lifecycle order.
    ///
    /// The one place the set is written down. `parse` rejects against it and
    /// the CLI advertises it, so help, errors, and parsing cannot drift apart.
    pub const VALUES: [Self; 3] = [Self::Open, Self::InProgress, Self::Closed];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::InProgress => "in progress",
            Self::Closed => "closed",
        }
    }

    /// Parse the wire form used by the CLI, the schema, and GraphQL alike.
    pub fn parse(value: &str) -> Result<Self> {
        Self::VALUES
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| {
                anyhow!(
                    "unknown state type {value:?}; use {}",
                    crate::domain::join_options(Self::VALUES.map(Self::as_str))
                )
            })
    }

    /// Whether issues in a state of this type count as closed.
    ///
    /// This is derived, not stored: `closed` is the only terminal type.
    pub fn is_closed(self) -> bool {
        self == Self::Closed
    }
}

impl fmt::Display for StateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How `issue list` selects which issues to return.
///
/// The three variants are mutually exclusive on the command line. Omitting a
/// selector means `Types` over the non-closed types: a listing that hides
/// resolved work by default is worth more than perfect orthogonality with
/// `All`.
pub enum IssueListSelector {
    Types(Vec<StateType>),
    States(Vec<String>),
    All,
}

impl Default for IssueListSelector {
    fn default() -> Self {
        Self::Types(vec![StateType::Open, StateType::InProgress])
    }
}

impl IssueListSelector {
    pub(crate) fn includes(&self, state_type: StateType, state_name: &str) -> bool {
        match self {
            Self::Types(types) => types.contains(&state_type),
            Self::States(names) => names.iter().any(|name| name == state_name),
            Self::All => true,
        }
    }
}

pub enum LeaseOutcome {
    Acquired(String),
    AlreadyLeased,
}
