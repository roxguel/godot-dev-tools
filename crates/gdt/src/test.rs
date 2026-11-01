//! `gdt test`: run the project's tests headless with the right Godot.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};
use clap::Args;
use godot_dev_tools::{
    Dirs, MANIFEST_FILE, Manifest, Outcome, Platform, Summary, build_plan, detect_runner,
    find_console_executable, find_project_root, judge, parse_junit, res_path,
};

use crate::select::resolve_for;
use crate::{Ctx, ExitWith, UsageError};

/// The exit code when tests fail (docs/specs/cli.md).
const TESTS_FAILED: i32 = 10;

#[derive(Args)]
pub struct TestArgs {
    /// Folders or test files to run (default: `paths` in [test] of gdt.toml, or `test`)
    paths: Vec<PathBuf>,
    /// Use this runner instead of the one found: gut, gdunit4 or script
    #[arg(long, value_name = "NAME")]
    runner: Option<String>,
    /// Copy the JUnit report to this file (for CI)
    #[arg(long, value_name = "FILE")]
    report: Option<PathBuf>,
}

/// Imports the project once, so its `class_name`s are known. A fresh clone has
/// no `.godot` folder, and without this GUT cannot even load.
fn import_if_needed(executable: &Path, project: &Path, ctx: &Ctx) -> Result<()> {
    if project
        .join(".godot")
        .join("global_script_class_cache.cfg")
        .is_file()
    {
        return Ok(());
    }
    ctx.info("Importing the project (first run)...");
    let output = Command::new(executable)
        .args(["--headless", "--path"])
        .arg(project)
        .arg("--import")
        .output()
        .with_context(|| format!("could not start {}", executable.display()))?;
    log::debug!(
        "import output:\n{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if !output.status.success() {
        bail!(
            "importing the project failed (exit code {}):\n{}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn describe(summary: &Summary) -> String {
    format!(
        "Tests: {}, passed: {}, failed: {}, skipped: {}",
        summary.tests,
        summary.passed(),
        summary.failures + summary.errors,
        summary.skipped
    )
}

pub fn test(args: &TestArgs, ctx: &Ctx) -> Result<()> {
    let root = find_project_root(Path::new("."))
        .filter(|r| r.project_file.is_some())
        .ok_or_else(|| {
            UsageError(
                "this is not inside a Godot project (no project.godot found); run it from your project's folder"
                    .into(),
            )
        })?;
    let manifest_path = root.dir.join(MANIFEST_FILE);
    let manifest = if manifest_path.is_file() {
        Manifest::read(&manifest_path)?
    } else {
        Manifest::default()
    };
    let mut config = manifest.test;
    if let Some(runner) = &args.runner {
        config.runner = Some(runner.clone());
    }
    let runner = detect_runner(&root.dir, &config)?;

    // Paths typed on the command line are relative to where gdt was run; the
    // ones in gdt.toml are relative to the project.
    let cwd = std::env::current_dir().context("could not read the current folder")?;
    let paths: Vec<String> = if args.paths.is_empty() {
        config
            .paths
            .iter()
            .map(|p| res_path(&root.dir, &root.dir, p))
            .collect::<Result<_, _>>()?
    } else {
        args.paths
            .iter()
            .map(|p| res_path(&root.dir, &cwd, &p.to_string_lossy()))
            .collect::<Result<_, _>>()?
    };
    let plan = build_plan(&root.dir, runner, &config, &paths)?;

    let dirs = Dirs::detect()?;
    let resolved = resolve_for(ctx, &dirs, &root.dir)?;
    let build_dir = dirs.build_dir(&resolved.build);
    let executable = find_console_executable(&build_dir, Platform::current())
        .with_context(|| format!("no Godot executable found in {}", build_dir.display()))?;

    import_if_needed(&executable, &root.dir, ctx)?;
    if let Some(report) = &plan.report {
        if let Some(dir) = report.parent() {
            fs::create_dir_all(dir).context("could not create the report folder")?;
        }
        // An old report left behind would be read as this run's result.
        match fs::remove_file(report) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("could not delete the previous report {}", report.display())
                });
            }
        }
    }

    ctx.info(format!(
        "Running {runner} tests with Godot {} (from {})",
        resolved.build, resolved.source
    ));
    log::debug!("{} {:?}", executable.display(), plan.args);
    // Ctrl+C reaches Godot too; gdt waits for it and reports what happened.
    let _ = ctrlc::set_handler(|| {});
    let status = Command::new(&executable)
        .args(&plan.args)
        .current_dir(&root.dir)
        .status()
        .with_context(|| format!("could not start {}", executable.display()))?;
    let code = crate::system::exit_code(status);

    let report_text = plan
        .report
        .as_ref()
        .and_then(|p| fs::read_to_string(p).ok());
    let summary = report_text.as_deref().and_then(parse_junit);
    if let (Some(copy), Some(source)) = (&args.report, &plan.report) {
        if source.is_file() {
            if let Some(dir) = copy.parent().filter(|d| !d.as_os_str().is_empty()) {
                fs::create_dir_all(dir).context("could not create the report folder")?;
            }
            fs::copy(source, copy)
                .with_context(|| format!("could not write {}", copy.display()))?;
            ctx.info(format!("JUnit report: {}", copy.display()));
        } else {
            ctx.info("The runner wrote no JUnit report, so there is none to copy");
        }
    }

    if let Some(summary) = &summary {
        ctx.info(describe(summary));
    }
    match judge(runner, code, summary) {
        Outcome::Passed => Ok(()),
        Outcome::Failed => Err(ExitWith(TESTS_FAILED).into()),
        Outcome::NothingRan => bail!(
            "no tests were run: check that [test] paths (or the test folder) hold test files for {runner}"
        ),
        Outcome::RunnerFailed(code) => {
            eprintln!("The {runner} runner ended with exit code {code} before reporting results");
            Err(ExitWith(code).into())
        }
    }
}
