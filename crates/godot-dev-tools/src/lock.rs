//! The lockfile `gdt.lock` and the `.gitignore` block for addons (see
//! `docs/specs/addons.md`).
//!
//! The lockfile records exactly what was installed for each addon: the commit,
//! the folder inside the repository and a hash of the files, so that
//! `gdt addon install` reproduces it and notices local changes.

use std::fs;
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manifest::GitMode;
use crate::names::{addon_name_problem, normalize_subdir};

/// The name of the lockfile.
pub const LOCK_FILE: &str = "gdt.lock";

/// Errors from reading or writing the lockfile.
#[derive(Debug, thiserror::Error)]
pub enum LockError {
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

/// One installed addon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockedAddon {
    pub name: String,
    /// Where it came from, for example `github:bitwes/Gut`.
    pub source: String,
    /// The full commit.
    pub rev: String,
    /// The folder inside the repository that was copied.
    pub subdir: String,
    /// `sha256:<hex>` of the installed files.
    pub hash: String,
}

/// The lockfile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockFile {
    pub version: u32,
    #[serde(default, rename = "addon", skip_serializing_if = "Vec::is_empty")]
    pub addons: Vec<LockedAddon>,
}

impl Default for LockFile {
    fn default() -> Self {
        Self {
            version: 1,
            addons: Vec::new(),
        }
    }
}

impl LockFile {
    /// Reads the file at `path`; a missing file is an empty lockfile.
    pub fn load(path: &Path) -> Result<Self, LockError> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(source) => {
                return Err(LockError::Read {
                    path: path.display().to_string(),
                    source,
                });
            }
        };
        let invalid = |message: String| LockError::Invalid {
            path: path.display().to_string(),
            message,
        };
        let lock: Self = toml::from_str(&text).map_err(|e| invalid(e.to_string()))?;
        // The names become folders that gdt deletes and the subfolders are moved
        // out of archives, so a hand-edited file must not be able to point elsewhere.
        for addon in &lock.addons {
            if let Some(why) = addon_name_problem(&addon.name) {
                return Err(invalid(format!(
                    "the addon name `{}` cannot be used: {why}",
                    addon.name
                )));
            }
            if let Err(why) = normalize_subdir(&addon.subdir) {
                return Err(invalid(format!(
                    "the folder `{}` of {} cannot be used: {why}",
                    addon.subdir, addon.name
                )));
            }
        }
        Ok(lock)
    }

    /// Writes the file, with the addons sorted by name so diffs stay small.
    pub fn save(&mut self, path: &Path) -> Result<(), LockError> {
        self.addons.sort_by(|a, b| a.name.cmp(&b.name));
        let text = format!(
            "# Written by gdt. Commit it; do not edit it by hand.\n{}",
            toml::to_string(self).map_err(|e| LockError::Invalid {
                path: path.display().to_string(),
                message: e.to_string(),
            })?
        );
        fs::write(path, text).map_err(|source| LockError::Write {
            path: path.display().to_string(),
            source,
        })
    }

    pub fn get(&self, name: &str) -> Option<&LockedAddon> {
        self.addons.iter().find(|a| a.name == name)
    }

    /// Adds the entry, or replaces the one with the same name.
    pub fn upsert(&mut self, addon: LockedAddon) {
        match self.addons.iter_mut().find(|a| a.name == addon.name) {
            Some(existing) => *existing = addon,
            None => self.addons.push(addon),
        }
    }

    /// Removes the entry; `false` if there was none.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.addons.len();
        self.addons.retain(|a| a.name != name);
        self.addons.len() != before
    }
}

/// A hash of every file under `dir`: their paths (with `/`) and contents, in
/// path order, so it does not depend on the order the files were written in.
pub fn hash_dir(dir: &Path) -> io::Result<String> {
    fn walk(
        dir: &Path,
        prefix: &str,
        out: &mut Vec<(String, std::path::PathBuf)>,
    ) -> io::Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().to_string();
            let relative = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(&path, &relative, out)?;
            } else {
                out.push((relative, path));
            }
        }
        Ok(())
    }

    let mut files = Vec::new();
    walk(dir, "", &mut files)?;
    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut hasher = Sha256::new();
    for (relative, path) in files {
        let bytes = fs::read(path)?;
        hasher.update(relative.as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_le_bytes());
        hasher.update(&bytes);
    }
    let digest = hasher.finalize();
    Ok(format!(
        "sha256:{}",
        digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

const IGNORE_START: &str = "# >>> gdt addons >>>";
const IGNORE_END: &str = "# <<< gdt addons <<<";

/// The lines the `.gitignore` block should hold for a mode and the managed addons.
pub fn ignore_lines(mode: GitMode, managed: &[&str]) -> Vec<String> {
    match mode {
        GitMode::Track => Vec::new(),
        GitMode::Ignore => vec!["/addons/".to_string()],
        GitMode::Managed => {
            let mut names: Vec<&str> = managed.to_vec();
            names.sort_unstable();
            names.dedup();
            names.iter().map(|n| format!("/addons/{n}/")).collect()
        }
    }
}

/// Returns `text` with gdt's marked block set to `lines`: added at the end if it
/// is new, replaced if it exists, removed when `lines` is empty. The rest of the
/// file is left byte for byte.
pub fn set_ignore_block(text: &str, lines: &[String]) -> String {
    let existing: Vec<&str> = text.split_inclusive('\n').collect();
    let start = existing.iter().position(|l| l.trim_end() == IGNORE_START);
    let end = start.and_then(|s| {
        existing[s..]
            .iter()
            .position(|l| l.trim_end() == IGNORE_END)
            .map(|e| s + e)
    });

    let block: String = if lines.is_empty() {
        String::new()
    } else {
        format!("{IGNORE_START}\n{}\n{IGNORE_END}\n", lines.join("\n"))
    };

    match (start, end) {
        (Some(s), Some(e)) => {
            // Also drop the blank line gdt put before a block it is removing.
            let from = if block.is_empty() && s > 0 && existing[s - 1].trim().is_empty() {
                s - 1
            } else {
                s
            };
            let head: String = existing[..from].concat();
            let tail: String = existing[e + 1..].concat();
            format!("{head}{block}{tail}")
        }
        _ if block.is_empty() => text.to_string(),
        _ => {
            let mut out = text.to_string();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push('\n');
                }
                out.push('\n');
            }
            out.push_str(&block);
            out
        }
    }
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

    fn locked(name: &str, rev: &str) -> LockedAddon {
        LockedAddon {
            name: name.into(),
            source: format!("github:o/{name}"),
            rev: rev.into(),
            subdir: format!("addons/{name}"),
            hash: "sha256:00".into(),
        }
    }

    #[test]
    fn the_lockfile_round_trips_sorted() {
        let dir = scratch("lock-roundtrip");
        let path = dir.join(LOCK_FILE);
        let mut lock = LockFile::default();
        lock.upsert(locked("zeta", "b".repeat(40).as_str()));
        lock.upsert(locked("alpha", "a".repeat(40).as_str()));
        lock.save(&path).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# Written by gdt"));
        assert!(text.find("alpha").unwrap() < text.find("zeta").unwrap());

        let loaded = LockFile::load(&path).unwrap();
        assert_eq!(loaded.addons.len(), 2);
        assert_eq!(loaded.get("alpha").unwrap().rev, "a".repeat(40));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn upsert_replaces_and_remove_removes() {
        let mut lock = LockFile::default();
        lock.upsert(locked("gut", "1"));
        lock.upsert(locked("gut", "2"));
        assert_eq!(lock.addons.len(), 1);
        assert_eq!(lock.get("gut").unwrap().rev, "2");
        assert!(lock.remove("gut"));
        assert!(!lock.remove("gut"));
    }

    #[test]
    fn a_missing_lockfile_is_empty_and_garbage_is_reported() {
        let dir = scratch("lock-missing");
        assert_eq!(
            LockFile::load(&dir.join(LOCK_FILE)).unwrap(),
            LockFile::default()
        );
        fs::write(dir.join(LOCK_FILE), "version = [").unwrap();
        let err = LockFile::load(&dir.join(LOCK_FILE)).unwrap_err();
        assert!(matches!(err, LockError::Invalid { .. }));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_directory_hash_sees_content_and_names_but_not_order() {
        let dir = scratch("hash");
        let a = dir.join("a");
        fs::create_dir_all(a.join("sub")).unwrap();
        fs::write(a.join("one.gd"), "one").unwrap();
        fs::write(a.join("sub").join("two.gd"), "two").unwrap();
        let first = hash_dir(&a).unwrap();
        assert!(first.starts_with("sha256:") && first.len() == 7 + 64);
        assert_eq!(hash_dir(&a).unwrap(), first, "stable");

        // The same files written in the other order hash the same.
        let b = dir.join("b");
        fs::create_dir_all(b.join("sub")).unwrap();
        fs::write(b.join("sub").join("two.gd"), "two").unwrap();
        fs::write(b.join("one.gd"), "one").unwrap();
        assert_eq!(hash_dir(&b).unwrap(), first);

        fs::write(b.join("one.gd"), "changed").unwrap();
        assert_ne!(hash_dir(&b).unwrap(), first, "content matters");
        fs::write(b.join("one.gd"), "one").unwrap();
        fs::rename(b.join("one.gd"), b.join("uno.gd")).unwrap();
        assert_ne!(hash_dir(&b).unwrap(), first, "names matter");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ignore_lines_follow_the_mode() {
        assert_eq!(ignore_lines(GitMode::Track, &["gut"]), Vec::<String>::new());
        assert_eq!(ignore_lines(GitMode::Ignore, &["gut"]), vec!["/addons/"]);
        assert_eq!(
            ignore_lines(GitMode::Managed, &["zed", "gut", "gut"]),
            vec!["/addons/gut/", "/addons/zed/"]
        );
    }

    #[test]
    fn the_ignore_block_is_added_replaced_and_removed() {
        let original = "*.tmp\n.godot/\n";
        let lines = vec!["/addons/gut/".to_string()];
        let added = set_ignore_block(original, &lines);
        assert!(added.starts_with(original));
        assert!(added.contains("# >>> gdt addons >>>\n/addons/gut/\n# <<< gdt addons <<<\n"));

        let more = vec!["/addons/gut/".to_string(), "/addons/zed/".to_string()];
        let replaced = set_ignore_block(&added, &more);
        assert_eq!(replaced.matches("# >>> gdt addons >>>").count(), 1);
        assert!(replaced.contains("/addons/zed/"));
        assert!(replaced.starts_with(original));

        assert_eq!(set_ignore_block(&replaced, &[]), original);
        assert_eq!(
            set_ignore_block(original, &[]),
            original,
            "nothing to remove"
        );
        assert_eq!(
            set_ignore_block(&added, &lines),
            added,
            "setting the same lines changes nothing"
        );
    }

    #[test]
    fn the_ignore_block_copes_with_empty_and_unterminated_files() {
        let lines = vec!["/addons/".to_string()];
        assert!(set_ignore_block("", &lines).starts_with("# >>> gdt addons >>>"));
        let added = set_ignore_block("a", &lines);
        assert!(added.starts_with("a\n\n# >>>"));
        assert_eq!(set_ignore_block(&added, &[]), "a\n");
    }

    #[test]
    fn names_and_subdirs_that_escape_are_refused() {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-lock-escape", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join(LOCK_FILE);
        let entry = |name: &str, subdir: &str| {
            format!(
                "version = 1
[[addon]]
name = \"{name}\"
source = \"github:o/r\"
rev = \"abc\"
subdir = \"{subdir}\"
hash = \"sha256:0\"
"
            )
        };
        for (name, subdir) in [
            ("..", "."),
            ("../..", "."),
            ("a/b", "."),
            ("con", "."),
            ("gut", ".."),
            ("gut", "/etc"),
        ] {
            fs::write(&path, entry(name, subdir)).unwrap();
            let err = LockFile::load(&path).unwrap_err();
            assert!(
                matches!(err, LockError::Invalid { .. }),
                "{name:?} {subdir:?}"
            );
        }
        fs::write(&path, entry("gut", "addons/gut")).unwrap();
        assert_eq!(LockFile::load(&path).unwrap().addons.len(), 1);
        let _ = fs::remove_dir_all(dir);
    }
}
