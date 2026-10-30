//! Reading what a `project.godot` says about the engine it needs.
//!
//! The file records `config/features`, for example
//! `config/features=PackedStringArray("4.7", "C#", "Forward Plus")`. The first
//! entry shaped like `MAJOR.MINOR` is the engine line and `C#` marks a .NET
//! project. The patch version is never recorded there (see ADR 0005).

use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// The file that marks a Godot project.
pub const PROJECT_FILE: &str = "project.godot";

/// Errors from finding or reading a project file.
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("no {PROJECT_FILE} found in {start} or any of its parent folders")]
    NotFound { start: String },
}

/// Finds the `project.godot` that `start` refers to.
///
/// A file is returned as given. For a folder, that folder is searched first and
/// then each parent in turn, so the command works from anywhere inside a project
/// (ADR 0005). `.` and `..` are resolved lexically before searching.
pub fn find_project_file(start: &Path) -> Result<PathBuf, ProjectError> {
    let read_error = |source| ProjectError::Read {
        path: start.display().to_string(),
        source,
    };
    let meta = fs::metadata(start).map_err(read_error)?;
    if meta.is_file() {
        return Ok(start.to_path_buf());
    }

    let absolute = std::path::absolute(start).map_err(read_error)?;
    let folder = normalize(&absolute);
    folder
        .ancestors()
        .map(|dir| dir.join(PROJECT_FILE))
        .find(|candidate| candidate.is_file())
        .ok_or_else(|| ProjectError::NotFound {
            start: folder.display().to_string(),
        })
}

/// Removes `.` and resolves `..` without touching the filesystem, so that the
/// parents searched are those of the folder the user meant.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
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

    /// A scratch folder under the system temp dir, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn dir(&self, rel: &str) -> PathBuf {
            let p = self.0.join(rel);
            fs::create_dir_all(&p).unwrap();
            p
        }

        fn project(&self, rel: &str) -> PathBuf {
            let p = self.dir(rel).join(PROJECT_FILE);
            fs::write(&p, "config/features=PackedStringArray(\"4.7\")\n").unwrap();
            p
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn finds_the_project_in_the_given_folder() {
        let s = Scratch::new("here");
        let project = s.project("game");
        assert_eq!(find_project_file(&s.0.join("game")).unwrap(), project);
    }

    #[test]
    fn finds_the_project_in_a_parent_folder() {
        let s = Scratch::new("parent");
        let project = s.project("game");
        let deep = s.dir("game/scripts/ui");
        assert_eq!(find_project_file(&deep).unwrap(), project);
    }

    #[test]
    fn a_file_is_returned_as_given() {
        let s = Scratch::new("file");
        let project = s.project("game");
        assert_eq!(find_project_file(&project).unwrap(), project);
    }

    #[test]
    fn reports_when_there_is_no_project() {
        let s = Scratch::new("none");
        let empty = s.dir("empty");
        assert!(matches!(
            find_project_file(&empty),
            Err(ProjectError::NotFound { .. })
        ));
    }

    #[test]
    fn parent_components_are_resolved_before_searching() {
        let s = Scratch::new("dotdot");
        s.project("b/x");
        // `b/x/..` is `b`, which has no project: the one in `b/x` must not be found.
        let start = s.0.join("b").join("x").join("..");
        assert!(matches!(
            find_project_file(&start),
            Err(ProjectError::NotFound { .. })
        ));
    }

    #[test]
    fn a_missing_path_is_a_read_error() {
        let s = Scratch::new("missing");
        assert!(matches!(
            find_project_file(&s.0.join("nope")),
            Err(ProjectError::Read { .. })
        ));
    }

    #[test]
    fn missing_file_is_an_error() {
        let err = EngineFeatures::from_file(Path::new("definitely/not/here/project.godot"));
        assert!(matches!(err, Err(ProjectError::Read { .. })));
    }
}
