//! The project file `gdt.toml` (see `docs/specs/version-management.md`).
//!
//! For now it records which Godot build the project uses:
//!
//! ```toml
//! [godot]
//! version = "4.7.2"
//! ```

use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;
use toml_edit::{DocumentMut, value};

/// The name of the project file.
pub const MANIFEST_FILE: &str = "gdt.toml";

/// Errors from reading or writing `gdt.toml`.
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("could not read {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: io::Error,
    },
    #[error("{path} is not valid: {message}")]
    Invalid { path: String, message: String },
    #[error("could not write {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: io::Error,
    },
}

#[derive(Deserialize, Default)]
struct Raw {
    godot: Option<RawGodot>,
}

#[derive(Deserialize, Default)]
struct RawGodot {
    version: Option<String>,
}

/// What `gdt.toml` says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    /// `[godot] version`, as written (a version spec such as `4.7.2` or `4.7.2-mono`).
    pub version: Option<String>,
}

impl Manifest {
    /// Reads the file at `path`.
    pub fn read(path: &Path) -> Result<Self, ManifestError> {
        let text = fs::read_to_string(path).map_err(|source| ManifestError::Read {
            path: path.display().to_string(),
            source,
        })?;
        Self::parse(&text, path)
    }

    fn parse(text: &str, path: &Path) -> Result<Self, ManifestError> {
        let raw: Raw = toml::from_str(text).map_err(|e| ManifestError::Invalid {
            path: path.display().to_string(),
            message: e.to_string(),
        })?;
        Ok(Self {
            version: raw.godot.and_then(|g| g.version),
        })
    }
}

/// Sets `[godot] version` in the file at `path`, creating the file if needed.
/// Everything else in the file, comments included, is left as it was.
pub fn pin(path: &Path, version: &str) -> Result<(), ManifestError> {
    let invalid = |message: String| ManifestError::Invalid {
        path: path.display().to_string(),
        message,
    };
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(source) => {
            return Err(ManifestError::Read {
                path: path.display().to_string(),
                source,
            });
        }
    };
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| invalid(e.to_string()))?;
    if !doc.contains_table("godot") {
        doc["godot"] = toml_edit::table();
    }
    doc["godot"]["version"] = value(version);
    fs::write(path, doc.to_string()).map_err(|source| ManifestError::Write {
        path: path.display().to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_version_is_read() {
        let dir = scratch("manifest-read");
        let path = dir.join(MANIFEST_FILE);
        fs::write(&path, "[godot]\nversion = \"4.7.2-mono\"\n").unwrap();
        assert_eq!(
            Manifest::read(&path).unwrap().version.as_deref(),
            Some("4.7.2-mono")
        );

        fs::write(&path, "# nothing yet\n").unwrap();
        assert_eq!(Manifest::read(&path).unwrap(), Manifest::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unknown_tables_are_allowed() {
        let dir = scratch("manifest-unknown");
        let path = dir.join(MANIFEST_FILE);
        fs::write(
            &path,
            "[godot]\nversion = \"4.7\"\n[addons.gut]\ngithub = \"bitwes/Gut\"\n",
        )
        .unwrap();
        assert_eq!(
            Manifest::read(&path).unwrap().version.as_deref(),
            Some("4.7")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn garbage_is_reported_with_the_path() {
        let dir = scratch("manifest-bad");
        let path = dir.join(MANIFEST_FILE);
        fs::write(&path, "[godot\n").unwrap();
        let err = Manifest::read(&path).unwrap_err();
        assert!(matches!(err, ManifestError::Invalid { .. }));
        assert!(err.to_string().contains(MANIFEST_FILE));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pin_creates_the_file() {
        let dir = scratch("pin-new");
        let path = dir.join(MANIFEST_FILE);
        pin(&path, "4.7.2").unwrap();
        assert_eq!(
            Manifest::read(&path).unwrap().version.as_deref(),
            Some("4.7.2")
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn pin_keeps_everything_else() {
        let dir = scratch("pin-keep");
        let path = dir.join(MANIFEST_FILE);
        let original = "# my project\n[godot]\nversion = \"4.6.3\"  # old\n\n[addons.gut]\ngithub = \"bitwes/Gut\"\n";
        fs::write(&path, original).unwrap();
        pin(&path, "4.7.2").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my project"), "{text}");
        assert!(text.contains("version = \"4.7.2\""), "{text}");
        assert!(text.contains("[addons.gut]"), "{text}");
        assert!(text.contains("github = \"bitwes/Gut\""), "{text}");
        assert_eq!(
            Manifest::read(&path).unwrap().version.as_deref(),
            Some("4.7.2")
        );

        // A file with other tables but no [godot] gets the table added.
        fs::write(&path, "[addons.gut]\ngithub = \"bitwes/Gut\"\n").unwrap();
        pin(&path, "4.8-rc1").unwrap();
        assert_eq!(
            Manifest::read(&path).unwrap().version.as_deref(),
            Some("4.8-rc1")
        );
        assert!(fs::read_to_string(&path).unwrap().contains("[addons.gut]"));
        let _ = fs::remove_dir_all(dir);
    }
}
