//! Putting gdt's bin folder on `PATH`, only when the user asks for it with
//! `gdt shim install --modify-path` (and taking it out again with
//! `gdt shim uninstall --modify-path`).
//!
//! - **Windows:** the user `Path` in the registry is read as it is stored, with its
//!   type (`REG_EXPAND_SZ`) and its unexpanded `%VARIABLES%`, and written back
//!   the same way with the folder first. The previous value is saved first.
//! - **Linux and macOS:** a small `env` script in gdt's data folder adds the
//!   folder to `PATH`, and a marked block in the shell's profile loads it, the
//!   way `rustup` does. Only that block is ever touched.
//!
//! The text handling is separate from the registry and the files, so it is tested
//! on every platform.

#[cfg(any(not(windows), test))]
use std::fs;
#[cfg(any(not(windows), test))]
use std::io;
#[cfg(any(not(windows), test))]
use std::path::{Path, PathBuf};

/// What happened to the folder on `PATH` or in a profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Added,
    MovedToFront,
    AlreadyFirst,
    Removed,
    NotPresent,
}

impl Change {
    /// Whether anything was written.
    pub fn is_change(self) -> bool {
        !matches!(self, Self::AlreadyFirst | Self::NotPresent)
    }
}

// ---- Windows-style PATH values ---------------------------------------------

#[cfg(any(windows, test))]
fn normalize(entry: &str) -> String {
    entry.trim_end_matches(['\\', '/']).to_ascii_lowercase()
}

#[cfg(any(windows, test))]
fn same_entry(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

#[cfg(any(windows, test))]
fn entries(value: &str) -> Vec<&str> {
    if value.is_empty() {
        Vec::new()
    } else {
        value.split(';').collect()
    }
}

/// Puts `entry` first in a `;`-separated `PATH` value, leaving every other entry
/// exactly as it was written (variables such as `%USERPROFILE%` stay unexpanded).
#[cfg(any(windows, test))]
pub fn put_first(value: &str, entry: &str) -> (String, Change) {
    let parts = entries(value);
    match parts.iter().position(|p| same_entry(p, entry)) {
        Some(0) => (value.to_string(), Change::AlreadyFirst),
        Some(index) => {
            let mut rest = parts.clone();
            rest.remove(index);
            (format!("{entry};{}", rest.join(";")), Change::MovedToFront)
        }
        None if value.is_empty() => (entry.to_string(), Change::Added),
        None => (format!("{entry};{value}"), Change::Added),
    }
}

/// Removes every occurrence of `entry` from a `;`-separated `PATH` value.
#[cfg(any(windows, test))]
pub fn take_out(value: &str, entry: &str) -> (String, Change) {
    let parts = entries(value);
    if !parts.iter().any(|p| same_entry(p, entry)) {
        return (value.to_string(), Change::NotPresent);
    }
    let kept: Vec<&str> = parts
        .into_iter()
        .filter(|p| !same_entry(p, entry))
        .collect();
    (kept.join(";"), Change::Removed)
}

// ---- Windows: the registry ----------------------------------------------------

#[cfg(windows)]
pub mod windows {
    use super::{Change, put_first, take_out};
    use std::borrow::Cow;
    use std::io;
    use std::process::{Command, Stdio};

    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, RegType};
    use winreg::{RegKey, RegValue};

    /// The key that holds the user's environment variables.
    pub const USER_ENVIRONMENT: &str = "Environment";

    fn decode(bytes: &[u8]) -> String {
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
            .trim_end_matches('\0')
            .to_string()
    }

    fn encode(text: &str) -> Vec<u8> {
        text.encode_utf16()
            .chain(std::iter::once(0))
            .flat_map(u16::to_le_bytes)
            .collect()
    }

    fn read(key: &RegKey) -> io::Result<(String, RegType)> {
        match key.get_raw_value("Path") {
            Ok(value) => Ok((decode(&value.bytes), value.vtype)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Ok((String::new(), RegType::REG_EXPAND_SZ))
            }
            Err(e) => Err(e),
        }
    }

    /// The raw user `Path`, for reading only.
    pub fn read_path(subkey: &str) -> io::Result<String> {
        let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(subkey, KEY_READ)?;
        Ok(read(&key)?.0)
    }

    /// Puts `entry` first in (`add`) or removes it from the user `Path` under
    /// `subkey`. `before_write` gets the previous value just before anything is
    /// written, so it can be saved; if it fails nothing is written.
    pub fn edit_path(
        subkey: &str,
        entry: &str,
        add: bool,
        before_write: &dyn Fn(&str) -> io::Result<()>,
    ) -> io::Result<Change> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(subkey, KEY_READ | KEY_WRITE)?;
        let (previous, vtype) = read(&key)?;
        if !matches!(vtype, RegType::REG_SZ | RegType::REG_EXPAND_SZ) {
            return Err(io::Error::other(
                "the user Path is not a text value, so gdt will not change it",
            ));
        }
        let (new, change) = if add {
            put_first(&previous, entry)
        } else {
            take_out(&previous, entry)
        };
        if change.is_change() {
            before_write(&previous)?;
            key.set_raw_value(
                "Path",
                &RegValue {
                    bytes: Cow::Owned(encode(&new)),
                    vtype,
                },
            )?;
        }
        Ok(change)
    }

    /// Tells Windows the environment changed, so programs started afterwards from
    /// Explorer see it. Setting a user variable through .NET broadcasts the
    /// change; deleting one that does not exist writes nothing else. Terminals
    /// that are already open keep their old environment until they are restarted.
    pub fn notify_environment_changed() -> bool {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "[Environment]::SetEnvironmentVariable('GDT_NOTIFY_ENV_CHANGE', $null, 'User')",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

// ---- Linux and macOS: shell profiles -----------------------------------------------

#[cfg(any(not(windows), test))]
pub const BLOCK_START: &str = "# >>> gdt >>>";
#[cfg(any(not(windows), test))]
pub const BLOCK_END: &str = "# <<< gdt <<<";
#[cfg(any(not(windows), test))]
const FISH_HEADER: &str = "# gdt shell setup (managed by gdt)";

#[cfg(any(not(windows), test))]
/// Single-quotes `text` for a POSIX shell.
fn sh_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[cfg(any(not(windows), test))]
/// Single-quotes `text` for fish.
fn fish_quote(text: &str) -> String {
    format!("'{}'", text.replace('\\', "\\\\").replace('\'', "\\'"))
}

#[cfg(any(not(windows), test))]
/// The script that adds `bin` to `PATH` once, for sh, bash and zsh.
pub fn env_script(bin: &Path) -> String {
    let bin = bin.to_string_lossy();
    let quoted = sh_quote(&bin);
    format!(
        "# gdt shell setup (managed by gdt; remove with `gdt shim uninstall --modify-path`)\n\
         case \":${{PATH}}:\" in\n\
         \x20   *:{quoted}:*) ;;\n\
         \x20   *) export PATH={quoted}:\"$PATH\" ;;\n\
         esac\n"
    )
}

#[cfg(any(not(windows), test))]
/// The fish equivalent, written as a whole file that gdt owns.
pub fn fish_script(bin: &Path) -> String {
    let quoted = fish_quote(&bin.to_string_lossy());
    format!(
        "{FISH_HEADER}\nif not contains -- {quoted} $PATH\n    set -gx PATH {quoted} $PATH\nend\n"
    )
}

#[cfg(any(not(windows), test))]
/// Adds the marked block that loads `env_file` to a profile's text; `None` if it
/// is already there.
pub fn add_block(text: &str, env_file: &Path) -> Option<String> {
    if text.contains(BLOCK_START) {
        return None;
    }
    let mut out = text.to_string();
    if !out.is_empty() {
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "{BLOCK_START}\n. {}\n{BLOCK_END}\n",
        sh_quote(&env_file.to_string_lossy())
    ));
    Some(out)
}

#[cfg(any(not(windows), test))]
/// Removes the marked block (and the blank line gdt put before it); `None` if
/// there is none. Everything else is left byte for byte.
pub fn remove_block(text: &str) -> Option<String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let start = lines.iter().position(|l| l.trim_end() == BLOCK_START)?;
    let end = start
        + lines[start..]
            .iter()
            .position(|l| l.trim_end() == BLOCK_END)?;
    let from = if start > 0 && lines[start - 1].trim().is_empty() {
        start - 1
    } else {
        start
    };
    let kept: String = lines[..from]
        .iter()
        .chain(&lines[end + 1..])
        .copied()
        .collect();
    Some(kept)
}

#[cfg(any(not(windows), test))]
/// The shells gdt knows how to set up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
    /// sh, dash, ksh and anything else: `~/.profile`
    Other,
}

#[cfg(any(not(windows), test))]
impl Shell {
    /// The shell named by a `$SHELL` value such as `/bin/zsh`.
    pub fn from_path(shell: &str) -> Self {
        match Path::new(shell).file_name().and_then(|n| n.to_str()) {
            Some("zsh") => Self::Zsh,
            Some("bash") => Self::Bash,
            Some("fish") => Self::Fish,
            _ => Self::Other,
        }
    }
}

#[cfg(any(not(windows), test))]
/// Where things are, for one profile edit.
pub struct UnixEnv {
    pub home: PathBuf,
    pub shell: Shell,
    /// `$ZDOTDIR`, where zsh keeps its files when it is set.
    pub zdotdir: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, where fish keeps its configuration when it is set.
    pub config_home: Option<PathBuf>,
    /// gdt's data folder, where the `env` script lives.
    pub data_dir: PathBuf,
    /// gdt's bin folder, the one that goes on `PATH`.
    pub bin_dir: PathBuf,
}

#[cfg(any(not(windows), test))]
impl UnixEnv {
    fn env_file(&self) -> PathBuf {
        self.data_dir.join("env")
    }

    fn fish_file(&self) -> PathBuf {
        self.config_home
            .clone()
            .unwrap_or_else(|| self.home.join(".config"))
            .join("fish")
            .join("conf.d")
            .join("gdt.fish")
    }

    fn zsh_file(&self) -> PathBuf {
        self.zdotdir
            .clone()
            .unwrap_or_else(|| self.home.clone())
            .join(".zshenv")
    }

    /// The profiles to edit when adding, for the user's shell.
    fn profiles_to_edit(&self) -> Vec<PathBuf> {
        match self.shell {
            Shell::Zsh => vec![self.zsh_file()],
            Shell::Bash => {
                let existing: Vec<PathBuf> = [".bashrc", ".bash_profile", ".profile"]
                    .iter()
                    .map(|f| self.home.join(f))
                    .filter(|p| p.is_file())
                    .collect();
                if existing.is_empty() {
                    vec![self.home.join(".profile")]
                } else {
                    existing
                }
            }
            Shell::Fish => Vec::new(),
            Shell::Other => vec![self.home.join(".profile")],
        }
    }

    /// Every profile gdt might have edited, for taking it out again.
    fn profiles_that_may_have_it(&self) -> Vec<PathBuf> {
        vec![
            self.zsh_file(),
            self.home.join(".zshenv"),
            self.home.join(".bashrc"),
            self.home.join(".bash_profile"),
            self.home.join(".profile"),
        ]
    }
}

#[cfg(any(not(windows), test))]
/// A file that was changed.
#[derive(Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: PathBuf,
    pub change: Change,
}

#[cfg(any(not(windows), test))]
/// Adds (`add`) or removes the gdt folder for the user's shell.
pub fn modify_unix(env: &UnixEnv, add: bool) -> io::Result<Vec<FileChange>> {
    let mut changes = Vec::new();
    if add {
        fs::create_dir_all(&env.data_dir)?;
        fs::write(env.env_file(), env_script(&env.bin_dir))?;

        if env.shell == Shell::Fish {
            let file = env.fish_file();
            let script = fish_script(&env.bin_dir);
            let change = if fs::read_to_string(&file).is_ok_and(|t| t == script) {
                Change::AlreadyFirst
            } else {
                if let Some(dir) = file.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&file, script)?;
                Change::Added
            };
            changes.push(FileChange { path: file, change });
        }
        for path in env.profiles_to_edit() {
            let text = fs::read_to_string(&path).unwrap_or_default();
            let change = match add_block(&text, &env.env_file()) {
                Some(new) => {
                    fs::write(&path, new)?;
                    Change::Added
                }
                None => Change::AlreadyFirst,
            };
            changes.push(FileChange { path, change });
        }
    } else {
        let mut seen: Vec<PathBuf> = Vec::new();
        for path in env.profiles_that_may_have_it() {
            if seen.contains(&path) {
                continue;
            }
            seen.push(path.clone());
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            if let Some(new) = remove_block(&text) {
                fs::write(&path, new)?;
                changes.push(FileChange {
                    path,
                    change: Change::Removed,
                });
            }
        }
        let fish = env.fish_file();
        if fs::read_to_string(&fish).is_ok_and(|t| t.starts_with(FISH_HEADER)) {
            fs::remove_file(&fish)?;
            changes.push(FileChange {
                path: fish,
                change: Change::Removed,
            });
        }
        match fs::remove_file(env.env_file()) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
    }
    Ok(changes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_folder_goes_first_and_the_rest_is_untouched() {
        let value = r"%USERPROFILE%\bin;C:\Tools;%SystemRoot%\System32";
        let (new, change) = put_first(value, r"C:\gdt\bin");
        assert_eq!(change, Change::Added);
        assert_eq!(
            new,
            r"C:\gdt\bin;%USERPROFILE%\bin;C:\Tools;%SystemRoot%\System32"
        );
    }

    #[test]
    fn adding_is_idempotent_and_moves_it_to_the_front() {
        let (once, _) = put_first(r"C:\a;C:\b", r"C:\gdt\bin");
        let (twice, change) = put_first(&once, r"C:\gdt\bin");
        assert_eq!(change, Change::AlreadyFirst);
        assert_eq!(twice, once);

        let (moved, change) = put_first(r"C:\a;C:\GDT\bin\;C:\b", r"C:\gdt\bin");
        assert_eq!(change, Change::MovedToFront);
        assert_eq!(moved, r"C:\gdt\bin;C:\a;C:\b");
    }

    #[test]
    fn an_empty_path_just_gets_the_entry() {
        assert_eq!(
            put_first("", r"C:\gdt\bin"),
            (r"C:\gdt\bin".to_string(), Change::Added)
        );
    }

    #[test]
    fn removing_takes_out_every_copy_and_nothing_else() {
        let (new, change) = take_out(r"C:\gdt\bin;C:\a;c:\GDT\BIN\;%X%\b", r"C:\gdt\bin");
        assert_eq!(change, Change::Removed);
        assert_eq!(new, r"C:\a;%X%\b");
        assert_eq!(
            take_out(r"C:\a", r"C:\gdt\bin"),
            (r"C:\a".to_string(), Change::NotPresent)
        );
    }

    #[test]
    fn add_then_remove_gives_back_the_original() {
        let original = r"%USERPROFILE%\bin;C:\Tools;";
        let (added, _) = put_first(original, r"C:\gdt\bin");
        let (back, _) = take_out(&added, r"C:\gdt\bin");
        assert_eq!(back, original);
    }

    #[test]
    fn the_profile_block_is_added_once_and_removed_cleanly() {
        let env = Path::new("/home/u/.local/share/gdt/env");
        let original = "export A=1\nalias ll='ls -l'\n";
        let added = add_block(original, env).unwrap();
        assert!(added.starts_with(original));
        assert!(added.contains(". '/home/u/.local/share/gdt/env'"));
        assert_eq!(add_block(&added, env), None, "not added twice");
        assert_eq!(remove_block(&added).unwrap(), original);
        assert_eq!(remove_block(original), None);
    }

    #[test]
    fn the_block_handles_empty_and_unterminated_files() {
        let env = Path::new("/d/env");
        let added = add_block("", env).unwrap();
        assert!(added.starts_with(BLOCK_START));
        assert_eq!(remove_block(&added).unwrap(), "");
        // No newline at the end of the file.
        let added = add_block("export A=1", env).unwrap();
        assert!(added.starts_with("export A=1\n\n"));
        assert_eq!(remove_block(&added).unwrap(), "export A=1\n");
        // A start marker without an end marker is left alone.
        assert_eq!(remove_block(&format!("{BLOCK_START}\nfoo\n")), None);
    }

    #[test]
    fn scripts_quote_paths_safely() {
        let script = env_script(Path::new("/home/o'brien/gdt/bin"));
        assert!(script.contains("'/home/o'\\''brien/gdt/bin'"), "{script}");
        let fish = fish_script(Path::new("/home/o'brien/bin"));
        assert!(fish.contains("'/home/o\\'brien/bin'"), "{fish}");
    }

    #[test]
    fn shells_are_told_apart() {
        assert_eq!(Shell::from_path("/bin/zsh"), Shell::Zsh);
        assert_eq!(Shell::from_path("/usr/bin/bash"), Shell::Bash);
        assert_eq!(Shell::from_path("/usr/local/bin/fish"), Shell::Fish);
        assert_eq!(Shell::from_path("/bin/dash"), Shell::Other);
        assert_eq!(Shell::from_path(""), Shell::Other);
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn env_for(home: &Path, shell: Shell) -> UnixEnv {
        UnixEnv {
            home: home.to_path_buf(),
            shell,
            zdotdir: None,
            config_home: None,
            data_dir: home.join("data").join("gdt"),
            bin_dir: home.join("data").join("gdt").join("bin"),
        }
    }

    #[test]
    fn bash_edits_the_profiles_that_exist_and_undoes_it() {
        let home = scratch("unix-bash");
        fs::write(home.join(".bashrc"), "export B=1\n").unwrap();
        fs::write(home.join(".profile"), "export P=1\n").unwrap();
        let env = env_for(&home, Shell::Bash);

        let changes = modify_unix(&env, true).unwrap();
        assert_eq!(changes.len(), 2);
        assert!(changes.iter().all(|c| c.change == Change::Added));
        assert!(
            fs::read_to_string(home.join(".bashrc"))
                .unwrap()
                .contains(BLOCK_START)
        );
        assert!(
            !home.join(".bash_profile").exists(),
            "files that did not exist are not created"
        );
        let script = fs::read_to_string(env.env_file()).unwrap();
        assert!(script.contains("export PATH="));

        // Again: nothing is added twice.
        let again = modify_unix(&env, true).unwrap();
        assert!(again.iter().all(|c| c.change == Change::AlreadyFirst));
        let text = fs::read_to_string(home.join(".bashrc")).unwrap();
        assert_eq!(text.matches(BLOCK_START).count(), 1);

        let removed = modify_unix(&env, false).unwrap();
        assert_eq!(removed.len(), 2);
        assert_eq!(
            fs::read_to_string(home.join(".bashrc")).unwrap(),
            "export B=1\n"
        );
        assert_eq!(
            fs::read_to_string(home.join(".profile")).unwrap(),
            "export P=1\n"
        );
        assert!(!env.env_file().exists());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn bash_creates_a_profile_when_it_has_none() {
        let home = scratch("unix-bash-none");
        let changes = modify_unix(&env_for(&home, Shell::Bash), true).unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, home.join(".profile"));
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn zsh_uses_zshenv_and_honours_zdotdir() {
        let home = scratch("unix-zsh");
        let mut env = env_for(&home, Shell::Zsh);
        let changes = modify_unix(&env, true).unwrap();
        assert_eq!(changes[0].path, home.join(".zshenv"));
        modify_unix(&env, false).unwrap();

        let zdot = home.join("zdot");
        fs::create_dir_all(&zdot).unwrap();
        env.zdotdir = Some(zdot.clone());
        let changes = modify_unix(&env, true).unwrap();
        assert_eq!(changes[0].path, zdot.join(".zshenv"));
        modify_unix(&env, false).unwrap();
        assert_eq!(fs::read_to_string(zdot.join(".zshenv")).unwrap(), "");
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn fish_gets_its_own_file_and_loses_it_again() {
        let home = scratch("unix-fish");
        let env = env_for(&home, Shell::Fish);
        let changes = modify_unix(&env, true).unwrap();
        assert_eq!(changes.len(), 1);
        let file = home
            .join(".config")
            .join("fish")
            .join("conf.d")
            .join("gdt.fish");
        assert_eq!(changes[0].path, file);
        assert!(fs::read_to_string(&file).unwrap().contains("set -gx PATH"));
        assert_eq!(
            modify_unix(&env, true).unwrap()[0].change,
            Change::AlreadyFirst
        );

        modify_unix(&env, false).unwrap();
        assert!(!file.exists());
        let _ = fs::remove_dir_all(home);
    }

    #[test]
    fn removing_when_nothing_was_added_does_nothing() {
        let home = scratch("unix-none");
        fs::write(home.join(".profile"), "export P=1\n").unwrap();
        assert_eq!(
            modify_unix(&env_for(&home, Shell::Other), false).unwrap(),
            vec![]
        );
        assert_eq!(
            fs::read_to_string(home.join(".profile")).unwrap(),
            "export P=1\n"
        );
        let _ = fs::remove_dir_all(home);
    }

    #[cfg(windows)]
    mod registry {
        use super::super::windows::*;
        use super::super::*;
        use std::borrow::Cow;
        use winreg::enums::{HKEY_CURRENT_USER, KEY_ALL_ACCESS, RegType};
        use winreg::{RegKey, RegValue};

        /// A throwaway key under HKCU, removed when dropped. The tests never touch
        /// the real `Environment` key.
        struct TestKey(String);

        impl TestKey {
            fn new(name: &str) -> Self {
                let path = format!(r"Software\gdt-test-{}-{name}", std::process::id());
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                let _ = hkcu.delete_subkey_all(&path);
                hkcu.create_subkey(&path).unwrap();
                Self(path)
            }

            fn set(&self, text: &str, vtype: RegType) {
                let key = RegKey::predef(HKEY_CURRENT_USER)
                    .open_subkey_with_flags(&self.0, KEY_ALL_ACCESS)
                    .unwrap();
                let bytes: Vec<u8> = text
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .flat_map(u16::to_le_bytes)
                    .collect();
                key.set_raw_value(
                    "Path",
                    &RegValue {
                        bytes: Cow::Owned(bytes),
                        vtype,
                    },
                )
                .unwrap();
            }

            fn raw(&self) -> (String, RegType) {
                let key = RegKey::predef(HKEY_CURRENT_USER)
                    .open_subkey_with_flags(&self.0, KEY_ALL_ACCESS)
                    .unwrap();
                let value = key.get_raw_value("Path").unwrap();
                let units: Vec<u16> = value
                    .bytes
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                (
                    String::from_utf16_lossy(&units)
                        .trim_end_matches('\0')
                        .to_string(),
                    value.vtype,
                )
            }
        }

        impl Drop for TestKey {
            fn drop(&mut self) {
                let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.0);
            }
        }

        #[test]
        fn the_registry_value_keeps_its_type_and_its_variables() {
            let key = TestKey::new("expand");
            let original = r"%USERPROFILE%\bin;C:\Tools";
            key.set(original, RegType::REG_EXPAND_SZ);

            let saved = std::cell::RefCell::new(String::new());
            let backup = |previous: &str| {
                *saved.borrow_mut() = previous.to_string();
                Ok(())
            };
            let change = edit_path(&key.0, r"C:\gdt\bin", true, &backup).unwrap();
            assert_eq!(change, Change::Added);
            assert_eq!(
                *saved.borrow(),
                original,
                "the previous value was handed over to be saved"
            );
            let (text, vtype) = key.raw();
            assert_eq!(text, format!(r"C:\gdt\bin;{original}"));
            assert!(
                matches!(vtype, RegType::REG_EXPAND_SZ),
                "the value type is preserved"
            );

            let again = edit_path(&key.0, r"C:\gdt\bin", true, &backup).unwrap();
            assert_eq!(again, Change::AlreadyFirst);

            let removed = edit_path(&key.0, r"C:\gdt\bin", false, &backup).unwrap();
            assert_eq!(removed, Change::Removed);
            assert_eq!(key.raw().0, original);
        }

        #[test]
        fn a_plain_string_value_stays_plain() {
            let key = TestKey::new("plain");
            key.set(r"C:\a", RegType::REG_SZ);
            edit_path(&key.0, r"C:\gdt\bin", true, &|_| Ok(())).unwrap();
            assert!(matches!(key.raw().1, RegType::REG_SZ));
        }

        #[test]
        fn a_failed_backup_means_nothing_is_written() {
            let key = TestKey::new("backup");
            key.set(r"C:\a", RegType::REG_EXPAND_SZ);
            let fail = |_: &str| Err(io::Error::other("disk full"));
            assert!(edit_path(&key.0, r"C:\gdt\bin", true, &fail).is_err());
            assert_eq!(key.raw().0, r"C:\a");
        }

        #[test]
        fn a_value_that_is_not_text_is_refused() {
            let key = TestKey::new("binary");
            key.set("x", RegType::REG_BINARY);
            assert!(edit_path(&key.0, r"C:\gdt\bin", true, &|_| Ok(())).is_err());
        }

        #[test]
        fn a_missing_path_value_is_created_as_expandable() {
            let key = TestKey::new("missing");
            let change = edit_path(&key.0, r"C:\gdt\bin", true, &|_| Ok(())).unwrap();
            assert_eq!(change, Change::Added);
            let (text, vtype) = key.raw();
            assert_eq!(text, r"C:\gdt\bin");
            assert!(matches!(vtype, RegType::REG_EXPAND_SZ));
        }
    }
}
