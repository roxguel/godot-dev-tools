//! Programs that ship with the operating system.

use std::path::PathBuf;

/// A program that ships with Windows, by its full path under the Windows folder.
/// Started by name, Windows looks in the folder of the running program and in the
/// current folder first, so a copy left there would run in its place.
#[cfg(windows)]
pub fn windows_program(relative: &str) -> PathBuf {
    let root = std::env::var_os("SystemRoot")
        .or_else(|| std::env::var_os("windir"))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    root.join(relative)
}

/// The program to start for `name`: on Windows the system's own `explorer.exe`;
/// anywhere else the name is left to the usual lookup.
pub fn file_manager(name: &str) -> PathBuf {
    #[cfg(windows)]
    if name == "explorer" {
        return windows_program("explorer.exe");
    }
    PathBuf::from(name)
}

/// The exit code to report for a finished child: its own, or `128 + N` when
/// signal `N` ended it, as a shell reports it, so a script can tell a kill from a
/// failure.
pub fn exit_code(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return 128 + signal;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_child_killed_by_a_signal_reports_128_plus_the_signal() {
        let status = std::process::Command::new("sh")
            .args(["-c", "kill -TERM $$"])
            .status()
            .unwrap();
        assert_eq!(exit_code(status), 143);
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 7"])
            .status()
            .unwrap();
        assert_eq!(exit_code(status), 7);
    }

    #[cfg(windows)]
    #[test]
    fn windows_programs_have_a_full_path() {
        let path = windows_program(r"System32\WindowsPowerShell\v1.0\powershell.exe");
        assert!(path.is_absolute(), "{path:?}");
        assert!(path.ends_with(r"System32\WindowsPowerShell\v1.0\powershell.exe"));
        assert!(path.is_file(), "{path:?} should exist on Windows");
        assert!(file_manager("explorer").is_absolute());
    }

    #[test]
    fn other_names_are_left_alone() {
        assert_eq!(file_manager("xdg-open"), PathBuf::from("xdg-open"));
        assert_eq!(file_manager("open"), PathBuf::from("open"));
    }
}
