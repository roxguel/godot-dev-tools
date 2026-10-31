//! Refuses to follow symbolic links in the project's own files.
//!
//! A cloned project can ship `addons`, `gdt.toml`, `gdt.lock` or `.gitignore` as
//! links to somewhere else (Git versions links). gdt creates, rewrites and deletes
//! what it finds there, so it must not follow them out of the project.

use std::fs;
use std::io;
use std::path::Path;

/// Fails if `path` itself is a symbolic link; a missing path is fine.
pub fn refuse_symlink(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_symlink() => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "{} is a symbolic link, which gdt does not follow; replace it with a real file or folder",
                path.display()
            ),
        )),
        _ => Ok(()),
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

    #[test]
    fn real_and_missing_paths_pass() {
        let dir = scratch("guard-ok");
        fs::write(dir.join("file"), "x").unwrap();
        assert!(refuse_symlink(&dir.join("file")).is_ok());
        assert!(refuse_symlink(&dir).is_ok());
        assert!(refuse_symlink(&dir.join("missing")).is_ok());
        let _ = fs::remove_dir_all(dir);
    }

    #[cfg(unix)]
    #[test]
    fn links_are_refused_even_when_dangling() {
        let dir = scratch("guard-link");
        std::os::unix::fs::symlink(&dir, dir.join("to-folder")).unwrap();
        std::os::unix::fs::symlink(dir.join("nowhere"), dir.join("dangling")).unwrap();
        for name in ["to-folder", "dangling"] {
            let error = refuse_symlink(&dir.join(name)).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{name}");
        }
        let _ = fs::remove_dir_all(dir);
    }
}
