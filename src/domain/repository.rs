use serde::Serialize;

/// A repository octa has recorded, with the counts that say how much work it
/// holds.
///
/// `name` is the stable, unique user-facing identity shared with Git
/// configuration. `path` is the repository's current canonical location and
/// can change after the Git repository moves.
#[derive(Debug, Serialize)]
pub struct Repository {
    pub name: String,
    pub path: String,
    pub created_at: String,
    pub updated_at: String,
    /// Issues counted by their state's type. Each field names its type
    /// exactly, so a count never has to be read as anything but "issues whose
    /// state carries this type". Closed issues are left out: the listing is
    /// there to say what work a repository still holds.
    pub open_issues: i64,
    pub in_progress_issues: i64,
}
