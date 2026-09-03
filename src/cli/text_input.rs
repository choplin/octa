//! Reusable text input sources for CLI commands.

use anyhow::{Context, Result};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Text supplied directly, through a file, or through explicit stdin.
pub(crate) enum TextInput {
    Value(String),
    File(PathBuf),
    Stdin,
}

impl TextInput {
    /// Build an optional input from a direct value and a file option.
    ///
    /// clap should make the options mutually exclusive. The check here keeps
    /// programmatic construction from silently preferring either source.
    pub(crate) fn from_options(
        value: Option<String>,
        file: Option<PathBuf>,
    ) -> Result<Option<Self>> {
        match (value, file) {
            (Some(value), None) => Ok(Some(Self::Value(value))),
            (None, Some(path)) if path == Path::new("-") => Ok(Some(Self::Stdin)),
            (None, Some(path)) => Ok(Some(Self::File(path))),
            (None, None) => Ok(None),
            (Some(_), Some(_)) => anyhow::bail!("text value cannot be used with a text file"),
        }
    }

    /// Resolve the selected source as UTF-8 without normalizing its contents.
    pub(crate) fn read(self, description: &str) -> Result<String> {
        let (bytes, source) = match self {
            Self::Value(value) => return Ok(value),
            Self::File(path) => {
                let bytes = std::fs::read(&path).with_context(|| {
                    format!("cannot read {description} from {}", path.display())
                })?;
                (bytes, path.display().to_string())
            }
            Self::Stdin => {
                let mut bytes = Vec::new();
                std::io::stdin()
                    .read_to_end(&mut bytes)
                    .with_context(|| format!("cannot read {description} from stdin"))?;
                (bytes, "stdin".to_owned())
            }
        };

        String::from_utf8(bytes)
            .with_context(|| format!("{description} from {source} is not valid UTF-8"))
    }
}
