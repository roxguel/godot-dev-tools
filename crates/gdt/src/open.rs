//! `gdt paths --open`: show a folder or file in the system's file manager.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use godot_dev_tools::{Dirs, Platform};

/// What `--open` can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Place {
    /// gdt's data folder
    Data,
    /// gdt's settings folder
    Config,
    /// The settings file, selected in its folder
    ConfigFile,
    /// The installed Godot builds
    Versions,
    /// Downloaded archives
    Cache,
    /// Shims and the gdt link
    Bin,
}

impl Place {
    fn path(self, dirs: &Dirs) -> PathBuf {
        match self {
            Self::Data => dirs.data_dir(),
            Self::Config => dirs.config_dir(),
            Self::ConfigFile => dirs.config_file(),
            Self::Versions => dirs.versions_dir(),
            Self::Cache => dirs.cache_dir(),
            Self::Bin => dirs.bin_dir(),
        }
    }

    fn is_file(self) -> bool {
        self == Self::ConfigFile
    }
}

/// The program and arguments that show `path` in the file manager. A file is
/// selected in its folder where the system can do that.
fn open_command(platform: Platform, path: &Path, is_file: bool) -> (String, Vec<OsString>) {
    match (platform, is_file) {
        (Platform::Windows, false) => ("explorer".into(), vec![path.into()]),
        (Platform::Windows, true) => {
            let mut select = OsString::from("/select,");
            select.push(path);
            ("explorer".into(), vec![select])
        }
        (Platform::MacOs, false) => ("open".into(), vec![path.into()]),
        (Platform::MacOs, true) => ("open".into(), vec!["-R".into(), path.into()]),
        (Platform::Linux, is_file) => {
            // `xdg-open` cannot select a file, so it opens the folder.
            let folder = if is_file {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            ("xdg-open".into(), vec![folder.into()])
        }
    }
}

/// Opens `place` and returns the path that was shown.
pub fn open_place(place: Place, dirs: &Dirs) -> Result<PathBuf> {
    let path = place.path(dirs);
    let folder_exists = if place.is_file() {
        path.parent().is_some_and(Path::is_dir)
    } else {
        path.is_dir()
    };
    if !path.exists() && !folder_exists {
        bail!("{} does not exist yet", path.display());
    }
    // A settings file that has not been written yet: show its folder instead.
    let is_file = place.is_file() && path.is_file();
    let target = if place.is_file() && !path.is_file() {
        path.parent().map(Path::to_path_buf).unwrap_or(path.clone())
    } else {
        path.clone()
    };

    let (program, args) = open_command(Platform::current(), &target, is_file);
    log::debug!("opening with {program} {args:?}");
    // File managers often exit with a non-zero status even when they worked
    // (Explorer does), so only a failure to start is an error.
    Command::new(crate::system::file_manager(&program))
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("could not start `{program}`"))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(s: &str) -> OsString {
        s.into()
    }

    #[test]
    fn folders_are_opened_in_the_file_manager() {
        let p = Path::new("/data/gdt");
        assert_eq!(
            open_command(Platform::Windows, p, false),
            ("explorer".into(), vec![os("/data/gdt")])
        );
        assert_eq!(
            open_command(Platform::MacOs, p, false),
            ("open".into(), vec![os("/data/gdt")])
        );
        assert_eq!(
            open_command(Platform::Linux, p, false),
            ("xdg-open".into(), vec![os("/data/gdt")])
        );
    }

    #[test]
    fn files_are_selected_where_the_system_can() {
        let p = Path::new("/cfg/gdt/config.toml");
        assert_eq!(
            open_command(Platform::Windows, p, true),
            ("explorer".into(), vec![os("/select,/cfg/gdt/config.toml")])
        );
        assert_eq!(
            open_command(Platform::MacOs, p, true),
            ("open".into(), vec![os("-R"), os("/cfg/gdt/config.toml")])
        );
        assert_eq!(
            open_command(Platform::Linux, p, true),
            ("xdg-open".into(), vec![os("/cfg/gdt")])
        );
    }
}
