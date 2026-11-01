//! Checks for names and paths that come from outside gdt (command line,
//! `gdt.toml`, `gdt.lock`) before they are used to build a path on disk.
//!
//! An addon name becomes the folder `addons/<name>`, which gdt creates and
//! deletes, so it must be one plain folder name that is valid on every platform.
//! A `subdir` names a folder inside a downloaded archive and must stay inside it.

/// The longest addon folder name accepted.
const MAX_NAME_LEN: usize = 100;

/// Names Windows reserves for devices, with or without an extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Letters, digits, `.`, `_`, `-` and spaces: the name is written into
/// `.gitignore` and printed, so characters a shell or Git treats specially
/// (`$ ( ) ; & ' ! # ~ [ ] { } * ?` and the like) are out.
fn has_forbidden_char(text: &str) -> bool {
    !text
        .chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | ' '))
}

fn is_reserved(component: &str) -> bool {
    let stem = component.split('.').next().unwrap_or(component);
    RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem))
}

/// Why `name` cannot be the folder of an addon, or `None` if it can.
pub fn addon_name_problem(name: &str) -> Option<&'static str> {
    if name.is_empty() {
        Some("it is empty")
    } else if name.len() > MAX_NAME_LEN {
        Some("it is too long")
    } else if name.starts_with('.') {
        Some("it starts with a dot")
    } else if name.starts_with(' ') || name.ends_with(' ') || name.ends_with('.') {
        Some("it starts or ends with a space, or ends with a dot")
    } else if has_forbidden_char(name) {
        Some("it may only have letters, digits, spaces, `.`, `_` and `-`")
    } else if is_reserved(name) {
        Some("it is a reserved name on Windows")
    } else {
        None
    }
}

/// Why a file or folder name inside an archive is dangerous to create on Windows,
/// or `None`. Windows treats `name:stream` as a hidden stream of another file,
/// drops a trailing dot or space, and sends `con`, `nul`, `com1`... (with any
/// extension) to a device instead of a file.
pub fn windows_file_name_problem(component: &str) -> Option<&'static str> {
    if component.contains(':') {
        Some("the `:` names a hidden stream of a file on Windows")
    } else if component.ends_with('.') || component.ends_with(' ') {
        Some("Windows drops a trailing dot or space from a name")
    } else if is_reserved(component) {
        Some("it is a reserved device name on Windows")
    } else {
        None
    }
}

/// The folder inside an archive that `subdir` names, in `/` form (`.` for the
/// whole archive), or why it cannot be used. Accepts relative paths of plain
/// folder names separated by `/` or `\`, with an optional leading `./` and
/// trailing separator.
pub fn normalize_subdir(subdir: &str) -> Result<String, &'static str> {
    let normalized = subdir.replace('\\', "/");
    if normalized.starts_with('/') {
        return Err("it must be relative to the repository, not an absolute path");
    }
    let mut rest = normalized.trim_end_matches('/');
    while let Some(stripped) = rest.strip_prefix("./") {
        rest = stripped.trim_start_matches('/');
    }
    if rest.is_empty() {
        return Err("it is empty");
    }
    if rest == "." {
        return Ok(".".to_string());
    }
    for component in rest.split('/') {
        if component.is_empty() || component == "." || component == ".." {
            return Err("it must not contain `.`, `..` or empty parts");
        }
        if component.chars().any(|c| c.is_control() || c == ':') {
            return Err("it must be relative to the repository, not an absolute path");
        }
    }
    Ok(rest.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_names_are_accepted() {
        for ok in [
            "gut",
            "Godot-Jolt",
            "a.b",
            "gd_unit4",
            "my addon",
            "console",
            "Godot_Jolt-2",
            "añadido",
        ] {
            assert_eq!(addon_name_problem(ok), None, "{ok:?}");
        }
    }

    #[test]
    fn names_that_escape_or_break_on_windows_are_refused() {
        let long = "a".repeat(101);
        for bad in [
            "",
            ".",
            "..",
            ".hidden",
            "a/b",
            "a\\b",
            "../x",
            "..\\x",
            "c:",
            "C:\\x",
            "con",
            "NUL",
            "aux.txt",
            "COM1",
            "lpt9.cfg",
            "trailing.",
            "trailing ",
            " leading",
            "q?",
            "s*",
            "a|b",
            "a\"b",
            "a<b",
            "tab\tname",
            "nul\0byte",
            "x$(touch pwned)",
            "a; rm -rf ~ #",
            "a`b`",
            "a&b",
            "it's",
            "!important",
            "a[1]",
            "{a,b}",
            "~",
            "50%",
            long.as_str(),
        ] {
            assert!(addon_name_problem(bad).is_some(), "{bad:?}");
        }
    }

    #[test]
    fn names_that_are_not_files_on_windows_are_found() {
        for ok in [
            "plugin.cfg",
            "gut.gd",
            ".gitignore",
            "a b.txt",
            "console.gd",
            "lpt10",
        ] {
            assert_eq!(windows_file_name_problem(ok), None, "{ok:?}");
        }
        for bad in [
            "a.txt:hidden",
            "c:",
            "nul",
            "NUL.txt",
            "Aux.tar.gz",
            "com1",
            "x.",
            "x ",
        ] {
            assert!(windows_file_name_problem(bad).is_some(), "{bad:?}");
        }
    }

    #[test]
    fn relative_subdirs_are_accepted_and_normalized() {
        for (given, want) in [
            (".", "."),
            ("./", "."),
            ("addons/gut", "addons/gut"),
            ("addons\\gut", "addons/gut"),
            ("addons/gut/", "addons/gut"),
            ("./addons/gut", "addons/gut"),
            ("x.y/z", "x.y/z"),
        ] {
            assert_eq!(normalize_subdir(given).as_deref(), Ok(want), "{given:?}");
        }
    }

    #[test]
    fn subdirs_that_leave_the_archive_are_refused() {
        for bad in [
            "",
            "/",
            "..",
            "../x",
            "a/../b",
            "a/..",
            "a/./b",
            "a//b",
            "C:/x",
            "C:\\x",
            "\\\\server\\share",
            "//server/share",
            "/etc",
            "/addons/gut",
            "a\0b",
            "..\\..",
        ] {
            assert!(normalize_subdir(bad).is_err(), "{bad:?}");
        }
    }
}
