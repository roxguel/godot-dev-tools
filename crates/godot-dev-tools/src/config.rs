//! The user's `config.toml` (see ADR 0004).

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, value};

use crate::version::Build;

/// Errors from reading or writing the configuration file.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
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

/// User settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// The build used when a project does not ask for one, as a folder name
    /// such as `4.7.2` or `4.7.2-mono`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_engine: Option<String>,
}

impl Config {
    /// Reads the file at `path`; a missing file means the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        toml::from_str(&text).map_err(|e| ConfigError::Invalid {
            path: path.display().to_string(),
            message: e.to_string(),
        })
    }

    /// Writes the file at `path`, creating its folder.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let write = |source| ConfigError::Write {
            path: path.display().to_string(),
            source,
        };
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(write)?;
        }
        // Edit what is there instead of replacing it, so comments and keys that
        // this version does not know (a newer gdt may have written them) survive.
        let mut doc: DocumentMut = fs::read_to_string(path)
            .ok()
            .and_then(|text| text.parse().ok())
            .unwrap_or_default();
        match &self.default_engine {
            Some(engine) => doc["default_engine"] = value(engine.as_str()),
            None => {
                doc.remove("default_engine");
            }
        }
        fs::write(path, doc.to_string()).map_err(write)
    }

    /// The default build, when one is set and readable.
    pub fn default_build(&self) -> Option<Build> {
        Build::from_dir_name(self.default_engine.as_deref()?).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_missing_file_is_the_default() {
        let dir = scratch("cfg-missing");
        assert_eq!(
            Config::load(&dir.join("config.toml")).unwrap(),
            Config::default()
        );
    }

    #[test]
    fn it_round_trips() {
        let dir = scratch("cfg-roundtrip");
        let path = dir.join("nested").join("config.toml");
        let config = Config {
            default_engine: Some("4.7.2-mono".into()),
        };
        config.save(&path).unwrap();
        let loaded = Config::load(&path).unwrap();
        assert_eq!(loaded, config);
        assert_eq!(loaded.default_build().unwrap().to_string(), "4.7.2-mono");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn saving_keeps_comments_and_unknown_keys() {
        let dir = scratch("cfg-keep");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(
            &path,
            "# my settings
default_engine = \"4.6\"
future_key = 7 # from a newer gdt
",
        )
        .unwrap();
        let mut config = Config::load(&path).unwrap();
        config.default_engine = Some("4.7.2".into());
        config.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("# my settings"), "{text}");
        assert!(text.contains("future_key = 7 # from a newer gdt"), "{text}");
        assert!(text.contains("default_engine = \"4.7.2\""), "{text}");

        config.default_engine = None;
        config.save(&path).unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("default_engine"), "{text}");
        assert!(text.contains("future_key"), "{text}");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn garbage_is_reported_with_the_path() {
        let dir = scratch("cfg-garbage");
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        fs::write(&path, "default_engine = [").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert!(matches!(err, ConfigError::Invalid { .. }));
        assert!(err.to_string().contains("config.toml"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unreadable_default_is_ignored() {
        let config = Config {
            default_engine: Some("banana".into()),
        };
        assert_eq!(config.default_build(), None);
    }
}
