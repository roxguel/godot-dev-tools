//! Where `gdt` keeps its files (ADR 0004).
//!
//! Everything lives in the user's standard directories, so no administrator
//! rights are needed. `GDT_HOME` replaces both the data and config directories
//! with one root.

use std::ffi::OsString;
use std::path::PathBuf;

use crate::version::Build;

const APP_DIR: &str = "gdt";

/// Errors from locating the storage directories.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StorageError {
    #[error("the environment variable {0} is not set; set it or set GDT_HOME")]
    MissingEnv(&'static str),
}

/// The operating system family, which decides the directory conventions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Windows,
    Linux,
    MacOs,
}

impl Platform {
    /// The platform this binary was built for.
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The directories `gdt` uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dirs {
    data: PathBuf,
    config: PathBuf,
}

impl Dirs {
    /// Locates the directories for the current platform and environment.
    pub fn detect() -> Result<Self, StorageError> {
        Self::resolve(Platform::current(), |key| std::env::var_os(key))
    }

    /// Locates the directories for `platform`, reading variables through `env`.
    /// Taking the platform and the environment as inputs keeps this testable
    /// on any machine.
    pub fn resolve(
        platform: Platform,
        env: impl Fn(&str) -> Option<OsString>,
    ) -> Result<Self, StorageError> {
        let var = |key: &str| env(key).filter(|v| !v.is_empty()).map(PathBuf::from);
        let required = |key: &'static str| var(key).ok_or(StorageError::MissingEnv(key));

        if let Some(root) = var("GDT_HOME") {
            return Ok(Self {
                data: root.clone(),
                config: root,
            });
        }

        match platform {
            Platform::Windows => Ok(Self {
                data: required("LOCALAPPDATA")?.join(APP_DIR),
                config: required("APPDATA")?.join(APP_DIR),
            }),
            Platform::MacOs => {
                let dir = required("HOME")?
                    .join("Library")
                    .join("Application Support")
                    .join(APP_DIR);
                Ok(Self {
                    data: dir.clone(),
                    config: dir,
                })
            }
            Platform::Linux => {
                // XDG says a relative path in these variables must be ignored.
                let xdg =
                    |key: &str| var(key).filter(|p| p.to_str().is_some_and(|s| s.starts_with('/')));
                let data = match xdg("XDG_DATA_HOME") {
                    Some(base) => base,
                    None => required("HOME")?.join(".local").join("share"),
                };
                let config = match xdg("XDG_CONFIG_HOME") {
                    Some(base) => base,
                    None => required("HOME")?.join(".config"),
                };
                Ok(Self {
                    data: data.join(APP_DIR),
                    config: config.join(APP_DIR),
                })
            }
        }
    }

    pub fn data_dir(&self) -> PathBuf {
        self.data.clone()
    }

    pub fn config_dir(&self) -> PathBuf {
        self.config.clone()
    }

    pub fn config_file(&self) -> PathBuf {
        self.config.join("config.toml")
    }

    /// One folder per installed build lives here.
    pub fn versions_dir(&self) -> PathBuf {
        self.data.join("versions")
    }

    /// The folder of one build, for example `versions/4.7.1-mono`.
    pub fn build_dir(&self, build: &Build) -> PathBuf {
        self.versions_dir().join(build.to_string())
    }

    /// Downloaded archives; safe to delete.
    pub fn cache_dir(&self) -> PathBuf {
        self.data.join("cache").join("downloads")
    }

    /// Where the shims and the `gdt` link live.
    pub fn bin_dir(&self) -> PathBuf {
        self.data.join("bin")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::version::{Flavor, Version};

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |key| {
            pairs
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn windows_uses_local_and_roaming_app_data() {
        let d = Dirs::resolve(
            Platform::Windows,
            env(&[
                ("LOCALAPPDATA", "C:\\Users\\u\\AppData\\Local"),
                ("APPDATA", "C:\\Users\\u\\AppData\\Roaming"),
            ]),
        )
        .unwrap();
        assert_eq!(
            d.data_dir(),
            PathBuf::from("C:\\Users\\u\\AppData\\Local").join("gdt")
        );
        assert_eq!(
            d.config_dir(),
            PathBuf::from("C:\\Users\\u\\AppData\\Roaming").join("gdt")
        );
    }

    #[test]
    fn linux_defaults_to_xdg_fallbacks() {
        let d = Dirs::resolve(Platform::Linux, env(&[("HOME", "/home/u")])).unwrap();
        assert_eq!(
            d.data_dir(),
            PathBuf::from("/home/u")
                .join(".local")
                .join("share")
                .join("gdt")
        );
        assert_eq!(
            d.config_dir(),
            PathBuf::from("/home/u").join(".config").join("gdt")
        );
    }

    #[test]
    fn linux_honours_absolute_xdg_variables() {
        let d = Dirs::resolve(
            Platform::Linux,
            env(&[
                ("HOME", "/home/u"),
                ("XDG_DATA_HOME", "/data"),
                ("XDG_CONFIG_HOME", "/cfg"),
            ]),
        )
        .unwrap();
        assert_eq!(d.data_dir(), PathBuf::from("/data").join("gdt"));
        assert_eq!(d.config_dir(), PathBuf::from("/cfg").join("gdt"));
    }

    #[test]
    fn linux_ignores_relative_xdg_variables() {
        let d = Dirs::resolve(
            Platform::Linux,
            env(&[("HOME", "/home/u"), ("XDG_DATA_HOME", "relative/dir")]),
        )
        .unwrap();
        assert_eq!(
            d.data_dir(),
            PathBuf::from("/home/u")
                .join(".local")
                .join("share")
                .join("gdt")
        );
    }

    #[test]
    fn macos_uses_application_support() {
        let d = Dirs::resolve(Platform::MacOs, env(&[("HOME", "/Users/u")])).unwrap();
        let expected = PathBuf::from("/Users/u")
            .join("Library")
            .join("Application Support")
            .join("gdt");
        assert_eq!(d.data_dir(), expected);
        assert_eq!(d.config_dir(), expected);
    }

    #[test]
    fn gdt_home_overrides_everything() {
        for platform in [Platform::Windows, Platform::Linux, Platform::MacOs] {
            let d = Dirs::resolve(platform, env(&[("GDT_HOME", "/opt/gdt")])).unwrap();
            assert_eq!(d.data_dir(), PathBuf::from("/opt/gdt"));
            assert_eq!(d.config_dir(), PathBuf::from("/opt/gdt"));
        }
    }

    #[test]
    fn empty_gdt_home_is_ignored() {
        let d = Dirs::resolve(
            Platform::Linux,
            env(&[("GDT_HOME", ""), ("HOME", "/home/u")]),
        )
        .unwrap();
        assert_eq!(
            d.config_dir(),
            PathBuf::from("/home/u").join(".config").join("gdt")
        );
    }

    #[test]
    fn missing_variables_are_reported_by_name() {
        assert_eq!(
            Dirs::resolve(Platform::Linux, env(&[])),
            Err(StorageError::MissingEnv("HOME"))
        );
        assert_eq!(
            Dirs::resolve(Platform::Windows, env(&[("APPDATA", "x")])),
            Err(StorageError::MissingEnv("LOCALAPPDATA"))
        );
    }

    #[test]
    fn derived_paths() {
        let d = Dirs::resolve(Platform::Linux, env(&[("GDT_HOME", "/g")])).unwrap();
        let build = Build {
            version: Version::from_tag("4.7.1-stable").unwrap(),
            flavor: Flavor::Dotnet,
        };
        assert_eq!(d.versions_dir(), PathBuf::from("/g").join("versions"));
        assert_eq!(
            d.build_dir(&build),
            PathBuf::from("/g").join("versions").join("4.7.1-mono")
        );
        assert_eq!(
            d.cache_dir(),
            PathBuf::from("/g").join("cache").join("downloads")
        );
        assert_eq!(d.bin_dir(), PathBuf::from("/g").join("bin"));
        assert_eq!(d.config_file(), PathBuf::from("/g").join("config.toml"));
    }
}
