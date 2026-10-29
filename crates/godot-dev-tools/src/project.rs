//! Reading what a `project.godot` says about the engine it needs.
//!
//! The file records `config/features`, for example
//! `config/features=PackedStringArray("4.7", "C#", "Forward Plus")`. The first
//! entry shaped like `MAJOR.MINOR` is the engine line and `C#` marks a .NET
//! project. The patch version is never recorded there (see ADR 0005).

use std::fs;
use std::io;
use std::path::Path;

/// Errors from reading a project file.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
}

/// The engine requirements found in `config/features`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct EngineFeatures {
    /// `(major, minor)` of the engine line, if the project records one.
    pub engine: Option<(u32, u32)>,
    /// Whether the project is a C# (.NET) project.
    pub dotnet: bool,
}

impl EngineFeatures {
    /// Reads and parses the `project.godot` at `path`.
    pub fn from_file(path: &Path) -> Result<Self, ProjectError> {
        let text = fs::read_to_string(path).map_err(|source| ProjectError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Ok(Self::parse(&text))
    }

    /// Parses the contents of a `project.godot`.
    pub fn parse(text: &str) -> Self {
        let mut features = Self::default();
        let Some(line) = text
            .lines()
            .map(str::trim)
            .find(|l| l.starts_with("config/features="))
        else {
            return features;
        };

        for entry in quoted_strings(line) {
            if entry == "C#" {
                features.dotnet = true;
            } else if features.engine.is_none() {
                features.engine = parse_major_minor(entry);
            }
        }
        features
    }
}

fn parse_major_minor(s: &str) -> Option<(u32, u32)> {
    let (major, minor) = s.split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// The double-quoted substrings of `line`, in order.
fn quoted_strings(line: &str) -> impl Iterator<Item = &str> {
    line.split('"').skip(1).step_by(2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_engine_line() {
        let text = "[application]\nconfig/features=PackedStringArray(\"4.7\", \"Forward Plus\")\n";
        let f = EngineFeatures::parse(text);
        assert_eq!(f.engine, Some((4, 7)));
        assert!(!f.dotnet);
    }

    #[test]
    fn detects_dotnet() {
        let text = "config/features=PackedStringArray(\"4.3\", \"C#\", \"Mobile\")";
        let f = EngineFeatures::parse(text);
        assert_eq!(f.engine, Some((4, 3)));
        assert!(f.dotnet);
    }

    #[test]
    fn missing_features_is_empty() {
        assert_eq!(
            EngineFeatures::parse("[application]\nconfig/name=\"x\"\n"),
            EngineFeatures::default()
        );
    }

    #[test]
    fn ignores_non_version_entries() {
        let f = EngineFeatures::parse("config/features=PackedStringArray(\"Mobile\")");
        assert_eq!(f.engine, None);
    }

    #[test]
    fn missing_file_is_an_error() {
        let err = EngineFeatures::from_file(Path::new("definitely/not/here/project.godot"));
        assert!(matches!(err, Err(ProjectError::Read { .. })));
    }
}
