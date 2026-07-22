use serde::Serialize;

/// A comment on an issue or pull request.
#[derive(Debug, Serialize)]
pub struct Comment {
    pub id: i64,
    pub body: String,
    pub created_at: String,
}
