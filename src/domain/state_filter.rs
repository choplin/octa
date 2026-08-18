use anyhow::{anyhow, Result};

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

impl StateFilter {
    /// Every value. See `StateType::VALUES`.
    pub const VALUES: [Self; 3] = [Self::Open, Self::Closed, Self::All];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Closed => "closed",
            Self::All => "all",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::VALUES
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| {
                anyhow!(
                    "unknown --state {value:?}; use {}",
                    crate::domain::join_options(Self::VALUES.map(Self::as_str))
                )
            })
    }
}
