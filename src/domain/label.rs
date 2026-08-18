use anyhow::{anyhow, Result};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Label {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct LabelGroup {
    pub name: String,
    pub selection: String,
}

/// How many labels of one group an Issue or Project may carry at once.
///
/// Stored as text, so the accepted set lives here rather than in the string
/// comparison that used to guard it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LabelSelection {
    Single,
    Multi,
}

impl LabelSelection {
    /// Every value. See `StateType::VALUES`.
    pub const VALUES: [Self; 2] = [Self::Single, Self::Multi];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Single => "single",
            Self::Multi => "multi",
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        Self::VALUES
            .into_iter()
            .find(|candidate| candidate.as_str() == value)
            .ok_or_else(|| {
                anyhow!(
                    "unknown selection {value:?}; use {}",
                    crate::domain::join_options(Self::VALUES.map(Self::as_str))
                )
            })
    }
}
