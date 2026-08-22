use serde::Serialize;

/// A repository octa has recorded, with the counts that say how much work it
/// holds.
///
/// `path` is the repository's Git common directory, and it is the repository's
/// identity: every worktree of one repository resolves to the same row. The
/// name is only a label derived from that path and is not unique, so the path
/// is what distinguishes two repositories that share a name.
#[derive(Debug, Serialize)]
pub struct Repository {
    pub name: String,
    pub path: String,
    pub created_at: String,
    /// Issues counted by their state's type. Each field names its type
    /// exactly, so a count never has to be read as anything but "issues whose
    /// state carries this type". Closed issues are left out: the listing is
    /// there to say what work a repository still holds.
    pub open_issues: i64,
    pub in_progress_issues: i64,
}
