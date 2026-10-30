//! The project file `gdt.toml` (see `docs/specs/version-management.md`).
//!
//! For now it records which Godot build the project uses:
//!
//! ```toml
//! [godot]
//! version = "4.7.2"
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use serde::Deserialize;
use toml_edit::{DocumentMut, Item, Table, value};

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
    /// `[addons]`: the key `git` and one sub-table per addon.
    #[serde(default)]
    addons: BTreeMap<String, toml::Value>,
    /// `[test]`.
    #[serde(default)]
    test: TestConfig,
}

/// The `[test]` table: how `gdt test` runs the project's tests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct TestConfig {
    /// `gut`, `gdunit4` or `script`; found from the installed addons when absent.
    pub runner: Option<String>,
    /// Folders or test files, relative to the project (default: `test`).
    #[serde(default)]
    pub paths: Vec<String>,
    /// For `runner = "script"`: a GDScript run headless that exits non-zero on failure.
    pub script: Option<String>,
}

/// What an `[addons.<name>]` table says: where the addon comes from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct AddonEntry {
    /// `owner/repo` on GitHub.
    pub github: Option<String>,
    /// A tag, branch or commit; the default branch when none is given.
    #[serde(rename = "ref")]
    pub git_ref: Option<String>,
    /// The folder inside the repository that holds the addon.
    pub subdir: Option<String>,
    /// Needed to work on the project but not shipped with the game.
    #[serde(default)]
    pub dev: bool,
}

/// How `addons/` relates to Git (`[addons] git`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GitMode {
    /// Only the folders gdt installed are ignored.
    #[default]
    Managed,
    /// Nothing is ignored.
    Track,
    /// The whole `addons/` folder is ignored.
    Ignore,
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
    /// The addons declared in `[addons.<name>]`.
    pub addons: BTreeMap<String, AddonEntry>,
    /// `[addons] git`.
    pub git_mode: GitMode,
    /// `[test]`.
    pub test: TestConfig,
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
        let invalid = |message: String| ManifestError::Invalid {
            path: path.display().to_string(),
            message,
        };
        let mut addons = BTreeMap::new();
        let mut git_mode = GitMode::default();
        for (key, item) in raw.addons {
            if key == "git" {
                git_mode = match item.as_str() {
                    Some("managed") => GitMode::Managed,
                    Some("track") => GitMode::Track,
                    Some("ignore") => GitMode::Ignore,
                    _ => {
                        return Err(invalid(
                            "[addons] git must be \"managed\", \"track\" or \"ignore\"".into(),
                        ));
                    }
                };
            } else {
                let entry: AddonEntry = item
                    .try_into()
                    .map_err(|e: toml::de::Error| invalid(format!("[addons.{key}]: {e}")))?;
                addons.insert(key, entry);
            }
        }
        Ok(Self {
            version: raw.godot.and_then(|g| g.version),
            addons,
            git_mode,
            test: raw.test,
        })
    }
}

fn load_document(path: &Path) -> Result<DocumentMut, ManifestError> {
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
    text.parse()
        .map_err(|e: toml_edit::TomlError| ManifestError::Invalid {
            path: path.display().to_string(),
            message: e.to_string(),
        })
}

fn save_document(path: &Path, doc: &DocumentMut) -> Result<(), ManifestError> {
    fs::write(path, doc.to_string()).map_err(|source| ManifestError::Write {
        path: path.display().to_string(),
        source,
    })
}

/// Adds or replaces `[addons.<name>]` in the file at `path` (created if needed),
/// leaving everything else, comments included, as it was.
pub fn set_addon(path: &Path, name: &str, entry: &AddonEntry) -> Result<(), ManifestError> {
    let mut doc = load_document(path)?;
    if !doc.contains_table("addons") {
        let mut table = Table::new();
        table.set_implicit(true);
        doc["addons"] = Item::Table(table);
    }
    let mut table = Table::new();
    if let Some(github) = &entry.github {
        table["github"] = value(github.as_str());
    }
    if let Some(git_ref) = &entry.git_ref {
        table["ref"] = value(git_ref.as_str());
    }
    if let Some(subdir) = &entry.subdir {
        table["subdir"] = value(subdir.as_str());
    }
    if entry.dev {
        table["dev"] = value(true);
    }
    doc["addons"][name] = Item::Table(table);
    save_document(path, &doc)
}

/// Removes `[addons.<name>]`; `false` if there was none.
pub fn remove_addon(path: &Path, name: &str) -> Result<bool, ManifestError> {
    let mut doc = load_document(path)?;
    let Some(addons) = doc.get_mut("addons").and_then(Item::as_table_mut) else {
        return Ok(false);
    };
    let removed = addons.remove(name).is_some();
    if removed {
        save_document(path, &doc)?;
    }
    Ok(removed)
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
    fn addons_and_the_git_mode_are_read() {
        let dir = scratch("manifest-addons");
        let path = dir.join(MANIFEST_FILE);
        fs::write(
            &path,
            "[godot]
version = \"4.7\"

[addons]
git = \"track\"

[addons.gut]
github = \"bitwes/Gut\"
ref = \"v9.4.0\"
subdir = \"addons/gut\"
dev = true

[addons.other]
github = \"o/r\"
",
        )
        .unwrap();
        let manifest = Manifest::read(&path).unwrap();
        assert_eq!(manifest.git_mode, GitMode::Track);
        assert_eq!(manifest.addons.len(), 2);
        let gut = &manifest.addons["gut"];
        assert_eq!(gut.github.as_deref(), Some("bitwes/Gut"));
        assert_eq!(gut.git_ref.as_deref(), Some("v9.4.0"));
        assert_eq!(gut.subdir.as_deref(), Some("addons/gut"));
        assert!(gut.dev);
        assert!(!manifest.addons["other"].dev);

        fs::write(
            &path,
            "[godot]
version = \"4.7\"
",
        )
        .unwrap();
        let bare = Manifest::read(&path).unwrap();
        assert!(bare.addons.is_empty());
        assert_eq!(bare.git_mode, GitMode::Managed);

        fs::write(
            &path,
            "[addons]
git = \"sometimes\"
",
        )
        .unwrap();
        assert!(matches!(
            Manifest::read(&path),
            Err(ManifestError::Invalid { .. })
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_test_table_is_read() {
        let dir = scratch("manifest-test");
        let path = dir.join(MANIFEST_FILE);
        fs::write(
            &path,
            "[test]
runner = \"script\"
paths = [\"test\", \"more/test_x.gd\"]
script = \"tools/tests/run_suites.gd\"
",
        )
        .unwrap();
        let test = Manifest::read(&path).unwrap().test;
        assert_eq!(test.runner.as_deref(), Some("script"));
        assert_eq!(test.paths, ["test", "more/test_x.gd"]);
        assert_eq!(test.script.as_deref(), Some("tools/tests/run_suites.gd"));

        fs::write(
            &path,
            "[godot]
version = \"4.7\"
",
        )
        .unwrap();
        assert_eq!(Manifest::read(&path).unwrap().test, TestConfig::default());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn set_and_remove_an_addon_keep_everything_else() {
        let dir = scratch("manifest-set-addon");
        let path = dir.join(MANIFEST_FILE);
        fs::write(
            &path,
            "# my project
[godot]
version = \"4.7.2\"  # pinned
",
        )
        .unwrap();

        let entry = AddonEntry {
            github: Some("bitwes/Gut".into()),
            git_ref: Some("v9.4.0".into()),
            subdir: Some("addons/gut".into()),
            dev: true,
        };
        set_addon(&path, "gut", &entry).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("# my project") && text.contains("# pinned"),
            "{text}"
        );
        assert!(text.contains("[addons.gut]"), "{text}");
        assert!(
            !text.contains(
                "[addons]
"
            ),
            "no empty parent header: {text}"
        );
        assert_eq!(Manifest::read(&path).unwrap().addons["gut"], entry);

        // Replacing keeps one table; a second addon is added next to it.
        set_addon(
            &path,
            "gut",
            &AddonEntry {
                dev: false,
                ..entry.clone()
            },
        )
        .unwrap();
        set_addon(
            &path,
            "zed",
            &AddonEntry {
                github: Some("o/zed".into()),
                ..AddonEntry::default()
            },
        )
        .unwrap();
        let manifest = Manifest::read(&path).unwrap();
        assert_eq!(manifest.addons.len(), 2);
        assert!(!manifest.addons["gut"].dev);

        assert!(remove_addon(&path, "gut").unwrap());
        assert!(!remove_addon(&path, "gut").unwrap());
        assert!(remove_addon(&path, "zed").unwrap());
        let after = fs::read_to_string(&path).unwrap();
        assert!(
            after.contains("# my project") && after.contains("version = \"4.7.2\""),
            "{after}"
        );
        assert!(Manifest::read(&path).unwrap().addons.is_empty());

        // A file that never existed is created, and removing from it is a no-op.
        let fresh = dir.join("fresh.toml");
        assert!(!remove_addon(&fresh, "x").unwrap());
        assert!(!fresh.exists());
        set_addon(&fresh, "x", &AddonEntry::default()).unwrap();
        assert!(Manifest::read(&fresh).unwrap().addons.contains_key("x"));
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
