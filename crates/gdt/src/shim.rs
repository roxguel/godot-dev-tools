//! The `godot` shim (ADR 0009, replacing the separate binary of ADR 0006).
//!
//! `gdt` is a multicall binary: when it starts under the name `godot` it does
//! nothing but pick the right Godot build and run it. `gdt shim install` puts a
//! copy (or, on Unix, a link) of `gdt` named `godot` in `<data>/bin`; with that
//! folder on `PATH`, a plain `godot ...` follows `gdt use`, `gdt.toml` and
//! `project.godot`.
//!
//! Shim mode must be invisible: it takes every argument for Godot (Godot has its
//! own `-v`, `-h` and `--version`), prints nothing on success and returns
//! Godot's exit code unchanged. Its own errors go to stderr prefixed `gdt:`.

use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use clap::Subcommand;
use godot_dev_tools::{
    Config, Dirs, Platform, Request, ResolveError, find_console_executable, find_project_root,
    godot_path_arg, resolve,
};

use crate::Ctx;

const SHIM_NAME: &str = "godot";

/// The file name of the shim on this platform.
fn shim_file_name() -> &'static str {
    if cfg!(windows) {
        "godot.exe"
    } else {
        SHIM_NAME
    }
}

/// Whether the program was started under the name `godot`.
pub fn invoked_as_shim() -> bool {
    env::args_os()
        .next()
        .is_some_and(|arg0| is_shim_name(&arg0))
}

fn is_shim_name(arg0: &OsStr) -> bool {
    Path::new(arg0)
        .file_stem()
        .is_some_and(|stem| stem.eq_ignore_ascii_case(SHIM_NAME))
}

/// Runs Godot as the shim and returns the exit code for the process.
pub fn run() -> i32 {
    match try_run() {
        Ok(code) => code,
        Err(failure) => {
            eprintln!("gdt: {}", failure.message);
            failure.code
        }
    }
}

struct Failure {
    message: String,
    code: i32,
}

impl Failure {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            code,
        }
    }
}

/// The exit code for a resolution error (docs/specs/cli.md).
fn resolve_failure(error: ResolveError) -> Failure {
    let code = match error {
        ResolveError::Version(_) => 2,
        ResolveError::NotInstalled { .. } | ResolveError::NothingSelected => 4,
        _ => 1,
    };
    Failure::new(code, error.to_string())
}

fn try_run() -> Result<i32, Failure> {
    let args: Vec<OsString> = env::args_os().skip(1).collect();
    let dirs = Dirs::detect().map_err(|e| Failure::new(1, e.to_string()))?;
    let installed = dirs
        .installed_builds()
        .map_err(|e| Failure::new(1, format!("could not read the installed builds: {e}")))?;
    let config = Config::load(&dirs.config_file()).map_err(|e| Failure::new(1, e.to_string()))?;

    let start = godot_path_arg(&args).unwrap_or_else(|| PathBuf::from("."));
    let root = find_project_root(&start);
    let version = env::var("GDT_VERSION").ok();
    let resolved = resolve(
        &Request {
            flag: None,
            env: version.as_deref(),
            project: root.as_ref(),
        },
        &installed,
        &config,
    )
    .map_err(resolve_failure)?;

    let dir = dirs.build_dir(&resolved.build);
    let executable = find_console_executable(&dir, Platform::current()).ok_or_else(|| {
        Failure::new(
            1,
            format!(
                "Godot {} is installed in {} but no executable was found there",
                resolved.build,
                dir.display()
            ),
        )
    })?;
    if is_this_program(&executable) {
        return Err(Failure::new(
            1,
            format!(
                "{} is the shim itself; reinstall Godot {}",
                executable.display(),
                resolved.build
            ),
        ));
    }
    start_godot(&executable, &args)
}

/// Guards against running ourselves in a loop.
fn is_this_program(executable: &Path) -> bool {
    match (
        env::current_exe().and_then(fs::canonicalize),
        fs::canonicalize(executable),
    ) {
        (Ok(me), Ok(other)) => me == other,
        _ => false,
    }
}

#[cfg(unix)]
fn start_godot(executable: &Path, args: &[OsString]) -> Result<i32, Failure> {
    use std::os::unix::process::CommandExt;
    // Replaces this process: the terminal, signals and exit code are Godot's own.
    let error = Command::new(executable).args(args).exec();
    Err(Failure::new(
        1,
        format!("could not start {}: {error}", executable.display()),
    ))
}

#[cfg(not(unix))]
fn start_godot(executable: &Path, args: &[OsString]) -> Result<i32, Failure> {
    // Ctrl+C reaches Godot too, since it shares the terminal; the shim waits for
    // it and returns its exit code instead of dying first and handing the prompt
    // back in the middle of its output.
    let _ = ctrlc::set_handler(|| {});
    let status = Command::new(executable)
        .args(args)
        .status()
        .map_err(|e| Failure::new(1, format!("could not start {}: {e}", executable.display())))?;
    Ok(status.code().unwrap_or(1))
}

#[derive(Subcommand)]
pub enum ShimCommand {
    /// Put the `godot` shim in gdt's bin folder (or refresh it)
    Install,
    /// Remove the `godot` shim
    Uninstall,
    /// Show whether the shim is installed, up to date, and the `godot` that runs
    Status,
}

pub fn command(command: &ShimCommand, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    match command {
        ShimCommand::Install => install(&dirs, ctx),
        ShimCommand::Uninstall => uninstall(&dirs, ctx),
        ShimCommand::Status => status(&dirs),
    }
}

fn shim_path(dirs: &Dirs) -> PathBuf {
    dirs.bin_dir().join(shim_file_name())
}

fn install(dirs: &Dirs, ctx: &Ctx) -> Result<()> {
    let me = env::current_exe().context("could not find the gdt executable")?;
    let target = shim_path(dirs);
    match install_shim(&dirs.bin_dir(), &me)? {
        Installed::AlreadyCurrent => {
            ctx.info(format!("The shim is up to date: {}", target.display()))
        }
        Installed::Installed => ctx.info(format!("Installed the shim: {}", target.display())),
    }
    print_path_report(dirs, ctx);
    Ok(())
}

fn uninstall(dirs: &Dirs, ctx: &Ctx) -> Result<()> {
    let target = shim_path(dirs);
    match fs::remove_file(&target) {
        Ok(()) => ctx.info(format!("Removed {}", target.display())),
        Err(e) if e.kind() == io::ErrorKind::NotFound => ctx.info("The shim is not installed"),
        Err(e) => return Err(locked_or(e, &target)),
    }
    Ok(())
}

/// A permission error on the shim usually means a Godot started through it is
/// still running (Windows will not replace a running program).
fn locked_or(error: io::Error, target: &Path) -> anyhow::Error {
    if error.kind() == io::ErrorKind::PermissionDenied {
        anyhow::anyhow!(
            "{} is in use: close the Godot windows that were started through it and try again",
            target.display()
        )
    } else {
        anyhow::Error::new(error).context(format!("could not update {}", target.display()))
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Installed {
    Installed,
    AlreadyCurrent,
}

/// Puts the shim in `bin_dir`, made from the program at `source`.
fn install_shim(bin_dir: &Path, source: &Path) -> Result<Installed> {
    fs::create_dir_all(bin_dir)
        .with_context(|| format!("could not create {}", bin_dir.display()))?;
    let target = bin_dir.join(shim_file_name());
    if shim_is_current(&target, source) {
        return Ok(Installed::AlreadyCurrent);
    }
    link_or_copy(source, &target).map_err(|e| locked_or(e, &target))?;
    Ok(Installed::Installed)
}

#[cfg(unix)]
fn link_or_copy(source: &Path, target: &Path) -> io::Result<()> {
    // A link follows `cargo install` and package upgrades, so it never goes stale.
    let source = fs::canonicalize(source)?;
    match fs::remove_file(target) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    std::os::unix::fs::symlink(source, target)
}

#[cfg(not(unix))]
fn link_or_copy(source: &Path, target: &Path) -> io::Result<()> {
    // Symbolic links need extra privileges on Windows, so the shim is a copy.
    fs::copy(source, target).map(|_| ())
}

/// Whether the installed shim is what `gdt` would install now.
fn shim_is_current(target: &Path, source: &Path) -> bool {
    #[cfg(unix)]
    {
        match (fs::read_link(target), fs::canonicalize(source)) {
            (Ok(link), Ok(source)) => link == source,
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (fs::read(target), fs::read(source)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

fn status(dirs: &Dirs) -> Result<()> {
    let target = shim_path(dirs);
    let me = env::current_exe().context("could not find the gdt executable")?;
    if target.exists() || fs::symlink_metadata(&target).is_ok() {
        let state = if shim_is_current(&target, &me) {
            "up to date"
        } else {
            "out of date: run `gdt shim install` to refresh it"
        };
        println!("shim:    {} ({state})", target.display());
    } else {
        println!("shim:    not installed; run `gdt shim install`");
    }

    let report = path_report(dirs);
    match report.position {
        Some(index) => println!("on PATH: yes (entry {} of {})", index + 1, report.entries),
        None => println!("on PATH: no ({})", dirs.bin_dir().display()),
    }
    match &report.winner {
        Some(winner) if report.winner_is_shim => {
            println!("godot:   {} (the shim)", winner.display());
        }
        Some(winner) => println!("godot:   {} (not the shim)", winner.display()),
        None => println!("godot:   none found on PATH"),
    }
    print_hints(dirs, &report);
    Ok(())
}

fn print_path_report(dirs: &Dirs, ctx: &Ctx) {
    let report = path_report(dirs);
    if report.position.is_some() && report.winner_is_shim {
        return;
    }
    if ctx.quiet {
        return;
    }
    print_hints(dirs, &report);
}

fn print_hints(dirs: &Dirs, report: &PathReport) {
    let bin = dirs.bin_dir();
    if report.position.is_none() {
        eprintln!();
        eprintln!("gdt does not change your PATH. To use the shim, add this folder to it:");
        eprintln!("  {}", bin.display());
        if cfg!(windows) {
            eprintln!(
                "Open \"Edit environment variables for your account\" (Start menu) and add it to \
                 the user variable Path. A `godot` in the system Path comes first, so check \
                 `gdt shim status`."
            );
        } else {
            eprintln!(
                "For example, in your shell profile: export PATH=\"{}:$PATH\"",
                bin.display()
            );
        }
    } else if let Some(winner) = report.winner.as_ref().filter(|_| !report.winner_is_shim) {
        eprintln!();
        eprintln!(
            "Another `godot` comes first on PATH and will be used instead of the shim: {}",
            winner.display()
        );
        eprintln!("Move {} before it, or remove that one.", bin.display());
    }
}

/// Where the shim stands in `PATH`.
#[derive(Debug, PartialEq, Eq)]
struct PathReport {
    /// Index of gdt's bin folder among the `PATH` entries.
    position: Option<usize>,
    entries: usize,
    /// The `godot` that would run.
    winner: Option<PathBuf>,
    winner_is_shim: bool,
}

fn path_report(dirs: &Dirs) -> PathReport {
    let path = env::var_os("PATH").unwrap_or_default();
    let entries: Vec<PathBuf> = env::split_paths(&path).collect();
    let names = candidate_names();
    analyze_path(&entries, &dirs.bin_dir(), &names, &|p| p.is_file())
}

/// The file names `godot` can have on this platform (`PATHEXT` on Windows).
fn candidate_names() -> Vec<String> {
    if cfg!(windows) {
        let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        pathext
            .split(';')
            .filter(|e| !e.is_empty())
            .map(|ext| format!("{SHIM_NAME}{}", ext.to_ascii_lowercase()))
            .collect()
    } else {
        vec![SHIM_NAME.to_string()]
    }
}

fn same_folder(a: &Path, b: &Path) -> bool {
    let norm = |p: &Path| {
        let s = p
            .to_string_lossy()
            .trim_end_matches(['/', '\\'])
            .to_string();
        if cfg!(windows) {
            s.to_ascii_lowercase()
        } else {
            s
        }
    };
    norm(a) == norm(b)
}

/// Finds gdt's bin folder in `entries` and the `godot` that would run.
fn analyze_path(
    entries: &[PathBuf],
    bin_dir: &Path,
    names: &[String],
    exists: &dyn Fn(&Path) -> bool,
) -> PathReport {
    let position = entries.iter().position(|e| same_folder(e, bin_dir));
    let winner = entries.iter().find_map(|dir| {
        names
            .iter()
            .map(|name| dir.join(name))
            .find(|candidate| exists(candidate))
    });
    let winner_is_shim = winner
        .as_ref()
        .and_then(|w| w.parent())
        .is_some_and(|dir| same_folder(dir, bin_dir));
    PathReport {
        position,
        entries: entries.len(),
        winner,
        winner_is_shim,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    #[test]
    fn the_shim_is_recognized_by_its_name() {
        assert!(is_shim_name(OsStr::new("godot")));
        assert!(is_shim_name(OsStr::new("godot.exe")));
        assert!(is_shim_name(OsStr::new(
            "/home/u/.local/share/gdt/bin/godot"
        )));
        assert!(is_shim_name(OsStr::new("C:\\gdt\\bin\\GODOT.EXE")));
        assert!(!is_shim_name(OsStr::new("gdt")));
        assert!(!is_shim_name(OsStr::new("gdt.exe")));
        assert!(!is_shim_name(OsStr::new("godot-dev-tools")));
        assert!(!is_shim_name(OsStr::new("Godot_v4.7.2-stable_win64.exe")));
    }

    #[test]
    fn path_analysis_finds_the_winner_and_the_position() {
        let bin = p("/gdt/bin");
        let entries = [p("/usr/bin"), p("/gdt/bin"), p("/opt/godot")];
        let names = vec!["godot".to_string()];
        let has = |f: &Path| f == Path::new("/gdt/bin/godot") || f == Path::new("/opt/godot/godot");
        let report = analyze_path(&entries, &bin, &names, &has);
        assert_eq!(report.position, Some(1));
        assert_eq!(report.entries, 3);
        assert_eq!(report.winner, Some(p("/gdt/bin/godot")));
        assert!(report.winner_is_shim);
    }

    #[test]
    fn another_godot_earlier_on_the_path_wins() {
        let bin = p("/gdt/bin");
        let entries = [p("/opt/godot"), p("/gdt/bin")];
        let names = vec!["godot".to_string()];
        let has = |f: &Path| f == Path::new("/gdt/bin/godot") || f == Path::new("/opt/godot/godot");
        let report = analyze_path(&entries, &bin, &names, &has);
        assert_eq!(report.position, Some(1));
        assert_eq!(report.winner, Some(p("/opt/godot/godot")));
        assert!(!report.winner_is_shim);
    }

    #[test]
    fn a_folder_that_is_not_on_the_path_is_reported() {
        let report = analyze_path(
            &[p("/usr/bin")],
            &p("/gdt/bin"),
            &["godot".to_string()],
            &|_| false,
        );
        assert_eq!(report.position, None);
        assert_eq!(report.winner, None);
        assert!(!report.winner_is_shim);
    }

    #[test]
    fn folders_compare_without_trailing_separators() {
        assert!(same_folder(Path::new("/a/b/"), Path::new("/a/b")));
        assert!(!same_folder(Path::new("/a/b"), Path::new("/a/c")));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn installing_is_idempotent_and_uninstalling_removes_it() {
        let dir = scratch("shim-install");
        let source = dir.join("gdt-source");
        fs::write(&source, b"pretend this is gdt").unwrap();
        let bin = dir.join("bin");

        assert_eq!(install_shim(&bin, &source).unwrap(), Installed::Installed);
        let target = bin.join(shim_file_name());
        assert!(fs::symlink_metadata(&target).is_ok());
        assert!(shim_is_current(&target, &source));
        assert_eq!(
            install_shim(&bin, &source).unwrap(),
            Installed::AlreadyCurrent
        );

        // A newer gdt makes the shim out of date, and installing refreshes it.
        fs::write(&source, b"a newer gdt, different bytes").unwrap();
        #[cfg(not(unix))]
        assert!(!shim_is_current(&target, &source));
        let other = dir.join("gdt-moved");
        fs::write(&other, b"gdt elsewhere").unwrap();
        assert!(!shim_is_current(&target, &other));
        assert_eq!(install_shim(&bin, &other).unwrap(), Installed::Installed);
        assert!(shim_is_current(&target, &other));

        fs::remove_file(&target).unwrap();
        let _ = fs::remove_dir_all(dir);
    }
}
