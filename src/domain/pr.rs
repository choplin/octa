use crate::domain::Comment;
use serde::Serialize;

/// A per-repository numbered discussion entity tied to a git branch.
#[derive(Debug, Serialize)]
pub struct Pr {
    pub repo: String,
    pub number: i64,
    pub title: String,
    pub body: String,
    pub branch: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct PrDetail {
    #[serde(flatten)]
    pub pr: Pr,
    pub comments: Vec<Comment>,
}

/// The PR fields needed to resume work from an issue detail view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PrRef {
    pub number: i64,
    pub title: String,
    pub branch: String,
    pub state: String,
}
