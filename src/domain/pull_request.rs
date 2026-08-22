use crate::domain::Comment;
use serde::Serialize;

/// A per-repository numbered discussion entity tied to a git branch.
#[derive(Debug, Serialize)]
pub struct PullRequest {
    #[serde(rename = "repo")]
    pub repository: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub branch: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PullRequestDetail {
    #[serde(flatten)]
    pub pull_request: PullRequest,
    pub comments: Vec<Comment>,
}

/// The pull request fields needed to resume work from an issue detail view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PullRequestRef {
    pub number: i64,
    pub title: String,
    pub branch: String,
    pub state: String,
}
