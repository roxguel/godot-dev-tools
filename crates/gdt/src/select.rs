//! Choosing which Godot build is used: `use`, `pin`, `which` and `uninstall`.

use std::env;
use std::path::{Path, PathBuf};

use anyhow::Result;
use clap::Args;
use godot_dev_tools::{
    Build, Config, Dirs, EngineError, Flavor, MANIFEST_FILE, Platform, Progress, Request, Resolved,
    Source, Target, Version, VersionSpec, fetch_releases, find_console_executable, find_executable,
    find_project_root, pick_installed, pin, removal_candidates, resolve, uninstall_build,
};
use serde_json::json;

use crate::progress::CliProgress;
use crate::{Ctx, UsageError, http_client, release_cache};

#[derive(Args)]
pub struct UninstallArgs {
    /// The build to remove, named exactly: 4.7.2, 4.8-rc1, or a line such as 4.7 when only one
    /// build of it is installed (add -mono for the .NET build)
    spec: String,
    /// The .NET build (same as the -mono suffix)
    #[arg(short, long)]
    mono: bool,
}

#[derive(Args)]
pub struct UseArgs {
    /// The installed build to make the default: 4.7.2, 4.7, latest, beta... Without it, shows
    /// the current default
    spec: Option<String>,
    /// The .NET build (same as the -mono suffix)
    #[arg(short, long)]
    mono: bool,
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Args)]
pub struct PinArgs {
    /// The build this project uses: 4.7.2, 4.7, latest... Stored as the exact version
    spec: String,
    /// The .NET build (same as the -mono suffix)
    #[arg(short, long)]
    mono: bool,
    /// Ignore the cached release list and ask GitHub again
    #[arg(long)]
    refresh: bool,
}

#[derive(Args)]
pub struct WhichArgs {
    /// A folder inside a project (default: the current folder)
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Show the editor executable instead of the one that prints to a terminal
    #[arg(short, long)]
    editor: bool,
}

fn parse_spec(text: &str, mono: bool) -> Result<VersionSpec> {
    let mut spec: VersionSpec = text.parse()?;
    if mono {
        spec.flavor = Flavor::Dotnet;
    }
    Ok(spec)
}

/// Decides which installed build applies to `start`, from the flag, the
/// environment, the project and the default engine.
pub fn resolve_for(ctx: &Ctx, dirs: &Dirs, start: &Path) -> Result<Resolved> {
    let installed = dirs.installed_builds()?;
    let config = Config::load(&dirs.config_file())?;
    let root = find_project_root(start);
    let env = env::var("GDT_VERSION").ok();
    let resolved = resolve(
        &Request {
            flag: ctx.godot.as_deref(),
            env: env.as_deref(),
            project: root.as_ref(),
        },
        &installed,
        &config,
    )?;
    log::debug!("using Godot {} from {}", resolved.build, resolved.source);
    Ok(resolved)
}

pub fn use_build(args: &UseArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let path = dirs.config_file();
    let mut config = Config::load(&path)?;

    let Some(text) = &args.spec else {
        match config.default_build() {
            Some(build) => println!("{build}"),
            None => ctx.info("No default engine is set. Choose one with `gdt use <version>`."),
        }
        return Ok(());
    };

    let spec = parse_spec(text, args.mono)?;
    let build = pick_installed(&spec, &dirs.installed_builds()?)
        .ok_or_else(|| EngineError::NotInstalled(text.trim().to_string()))?;
    config.default_engine = Some(build.to_string());
    config.save(&path)?;
    ctx.info(format!("Godot {build} is now the default engine"));
    if args.json {
        println!("{}", json!({ "default": build.to_string() }));
    }
    Ok(())
}

pub fn pin_build(args: &PinArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let spec = parse_spec(&args.spec, args.mono)?;

    let version: Version = match spec.target {
        Target::Exact(version) => version,
        _ => {
            let http = http_client();
            let cache = release_cache(&dirs);
            let mut progress = CliProgress::new(ctx.style);
            progress.begin("Fetching Godot releases", None);
            let source = if spec.needs_pre_releases() {
                Source::PreReleases
            } else {
                Source::Stable
            };
            let releases =
                fetch_releases(&http, Some(&cache), source, args.refresh, &mut progress)?;
            progress.finish(&format!("Found {} Godot releases", releases.len()));
            let versions: Vec<Version> = releases.iter().map(|r| r.version).collect();
            spec.pick(&versions)
                .ok_or_else(|| EngineError::VersionNotFound(args.spec.clone()))?
        }
    };
    let build = Build {
        version,
        flavor: spec.flavor,
    };

    let Some(root) = find_project_root(Path::new(".")) else {
        return Err(UsageError(
            "this is not inside a Godot project (no project.godot or gdt.toml found); \
             run `gdt pin` from your project's folder"
                .into(),
        )
        .into());
    };
    let path = root.dir.join(MANIFEST_FILE);
    pin(&path, &build.to_string())?;
    ctx.info(format!("Pinned Godot {build} in {}", path.display()));
    if !dirs.installed_builds()?.contains(&build) {
        ctx.info(format!(
            "It is not installed yet; run `gdt install {build}`"
        ));
    }
    Ok(())
}

pub fn which(args: &WhichArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let resolved = resolve_for(ctx, &dirs, &args.path)?;
    let dir = dirs.build_dir(&resolved.build);
    let platform = Platform::current();
    let executable = if args.editor {
        find_executable(&dir, platform)
    } else {
        find_console_executable(&dir, platform)
    };
    let Some(executable) = executable else {
        anyhow::bail!(
            "Godot {} is installed in {} but no executable was found there; reinstall it with `gdt uninstall {}` and `gdt install {}`",
            resolved.build,
            dir.display(),
            resolved.build,
            resolved.build
        );
    };
    println!("{}", executable.display());
    ctx.info(format!(
        "Godot {} (from {})",
        resolved.build, resolved.source
    ));
    Ok(())
}

pub fn uninstall(args: &UninstallArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let spec = parse_spec(&args.spec, args.mono)?;
    let installed = dirs.installed_builds()?;

    let Some(matches) = removal_candidates(&spec, &installed) else {
        return Err(UsageError(format!(
            "`{}` could match different builds over time; name the version exactly, \
             for example `gdt uninstall 4.7.2`",
            args.spec
        ))
        .into());
    };
    let build = match matches.as_slice() {
        [] => return Err(EngineError::NotInstalled(args.spec.trim().to_string()).into()),
        [one] => *one,
        many => {
            let names: Vec<String> = many.iter().map(|b| b.to_string()).collect();
            return Err(UsageError(format!(
                "{} installed builds match `{}`: {}; name one exactly",
                many.len(),
                args.spec,
                names.join(", ")
            ))
            .into());
        }
    };

    uninstall_build(&dirs, &build)?;
    ctx.info(format!("Removed Godot {build}"));

    let path = dirs.config_file();
    let mut config = Config::load(&path)?;
    if config.default_build() == Some(build) {
        config.default_engine = None;
        config.save(&path)?;
        ctx.info("It was the default engine; choose another with `gdt use <version>`");
    }
    Ok(())
}

#[derive(Args)]
pub struct CurrentArgs {
    /// A folder inside a project (default: the current folder)
    #[arg(default_value = ".")]
    path: PathBuf,
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

/// Prints the build in use here, ready for scripts: `gdt current`.
pub fn current(args: &CurrentArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let resolved = resolve_for(ctx, &dirs, &args.path)?;
    if args.json {
        println!(
            "{}",
            json!({
                "id": resolved.build.to_string(),
                "version": resolved.build.version.to_string(),
                "mono": resolved.build.flavor == Flavor::Dotnet,
                "from": resolved.source.to_string(),
            })
        );
    } else {
        println!("{}", resolved.build);
        ctx.info(format!("(from {})", resolved.source));
    }
    Ok(())
}
