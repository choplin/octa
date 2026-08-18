/// The `pr list` state selector.
///
/// Pull requests carry their own `state` column, unrelated to the configured
/// issue states. Issue listings select on the state type axis instead; see
/// `domain::issue::IssueListSelector`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateFilter {
    Open,
    Closed,
    All,
}
