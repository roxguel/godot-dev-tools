//! `gdt run` and `gdt open`: starting the right Godot.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{Context, Result};
use clap::Args;
use godot_dev_tools::{
    Dirs, Platform, find_console_executable, find_executable, find_project_root, godot_path_arg,
};

use crate::Ctx;
use crate::select::resolve_for;

#[derive(Args)]
pub struct RunArgs {
    /// A GDScript file to run headless (like `node file.js`), or the arguments for Godot
    /// itself. Write `--` first to pass Godot flags such as `--path`
    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        value_name = "SCRIPT_OR_ARGS"
    )]
    args: Vec<String>,
}

#[derive(Args)]
pub struct OpenArgs {
    /// A project.godot, or a folder inside a project (default: the current folder)
    path: Option<PathBuf>,
    /// Open Godot's project manager instead of a project
    #[arg(long)]
    manager: bool,
}

/// The arguments to give Godot for what was typed after `gdt run`.
///
/// A first argument ending in `.gd` is a script: it runs headless, and
/// anything after it goes to the script (as `OS.get_cmdline_user_args()`).
/// Anything else is passed to Godot untouched.
fn godot_args(typed: &[String]) -> Vec<String> {
    match typed.split_first() {
        Some((script, rest)) if script.ends_with(".gd") && !script.starts_with('-') => {
            let mut args = vec!["--headless".into(), "-s".into(), script.clone()];
            let rest = rest.strip_prefix(&["--".to_string()]).unwrap_or(rest);
            if !rest.is_empty() {
                args.push("--".into());
                args.extend(rest.iter().cloned());
            }
            args
        }
        _ => typed.to_vec(),
    }
}

pub fn run(args: &RunArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let godot = godot_args(&args.args);
    let start = godot_path_arg(&godot).unwrap_or_else(|| PathBuf::from("."));
    let resolved = resolve_for(ctx, &dirs, &start)?;

    let dir = dirs.build_dir(&resolved.build);
    let executable = find_console_executable(&dir, Platform::current())
        .with_context(|| format!("no Godot executable found in {}", dir.display()))?;
    log::debug!("running {} {godot:?}", executable.display());

    start_godot(&executable, &godot)
}

#[cfg(unix)]
fn start_godot(executable: &Path, args: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    // Replaces this process: the terminal, the signals (a `kill` sent to gdt
    // reaches Godot) and the exit code are Godot's own.
    let error = Command::new(executable).args(args).exec();
    Err(error).with_context(|| format!("could not start {}", executable.display()))
}

#[cfg(not(unix))]
fn start_godot(executable: &Path, args: &[String]) -> Result<()> {
    // Ctrl+C reaches Godot too, since it shares the terminal; gdt waits for it
    // and returns its exit code instead of dying first.
    let _ = ctrlc::set_handler(|| {});
    let status = Command::new(executable)
        .args(args)
        .status()
        .with_context(|| format!("could not start {}", executable.display()))?;
    match crate::system::exit_code(status) {
        0 => Ok(()),
        // The code as it is, not squeezed into a byte: a Windows crash code
        // such as 0xC0000005 must reach the caller unchanged.
        code => Err(crate::ExitWith(code).into()),
    }
}

pub fn open(args: &OpenArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let start = args.path.clone().unwrap_or_else(|| PathBuf::from("."));
    let start_folder = if start.is_file() {
        start
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        start.clone()
    };

    let root = find_project_root(&start_folder);
    let project = (!args.manager)
        .then(|| {
            root.as_ref()
                .and_then(|r| r.project_file.as_ref().map(|_| r.dir.clone()))
        })
        .flatten();
    if args.path.is_some() && project.is_none() && !args.manager {
        anyhow::bail!(
            "{} is not inside a Godot project (no project.godot found)",
            start.display()
        );
    }

    let resolved = resolve_for(ctx, &dirs, &start_folder)?;
    let dir = dirs.build_dir(&resolved.build);
    let executable = find_executable(&dir, Platform::current())
        .with_context(|| format!("no Godot executable found in {}", dir.display()))?;

    let mut command = Command::new(&executable);
    match &project {
        Some(folder) => {
            command.arg("--editor").arg("--path").arg(folder);
            ctx.info(format!(
                "Opening {} with Godot {} (from {})",
                folder.display(),
                resolved.build,
                resolved.source
            ));
        }
        None => {
            command.arg("--project-manager");
            ctx.info(format!(
                "Opening the project manager with Godot {}",
                resolved.build
            ));
        }
    }
    log::debug!("starting {command:?}");

    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    detach(&mut command);
    command
        .spawn()
        .with_context(|| format!("could not start {}", executable.display()))?;
    Ok(())
}

/// Lets the editor outlive the terminal it was started from.
#[cfg(windows)]
fn detach(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}

#[cfg(not(windows))]
fn detach(_command: &mut Command) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_script_runs_headless() {
        assert_eq!(
            godot_args(&v(&["tools/x.gd"])),
            v(&["--headless", "-s", "tools/x.gd"])
        );
    }

    #[test]
    fn arguments_after_a_script_go_to_the_script() {
        assert_eq!(
            godot_args(&v(&["x.gd", "a", "--flag"])),
            v(&["--headless", "-s", "x.gd", "--", "a", "--flag"])
        );
        // A `--` written by the user is not doubled.
        assert_eq!(
            godot_args(&v(&["x.gd", "--", "a"])),
            v(&["--headless", "-s", "x.gd", "--", "a"])
        );
    }

    #[test]
    fn anything_else_is_passed_through() {
        let raw = v(&["--path", ".", "-s", "x.gd", "--", "a"]);
        assert_eq!(godot_args(&raw), raw);
        assert_eq!(godot_args(&v(&["--version"])), v(&["--version"]));
        assert_eq!(godot_args(&[]), Vec::<String>::new());
        // A flag that merely ends in .gd is not a script.
        assert_eq!(
            godot_args(&v(&["--main-script.gd"])),
            v(&["--main-script.gd"])
        );
    }
}
