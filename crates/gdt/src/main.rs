mod progress;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use godot_dev_tools::{
    Build, Dirs, EngineFeatures, Flavor, HttpError, Progress, Release, ReleaseError, ResponseCache,
    Source, UreqClient, Version, fetch_releases, find_project_file,
};
use serde_json::json;

use progress::{CliProgress, ProgressArg, Style};

/// How long release lists are reused before asking GitHub again.
const RELEASE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);

/// Godot Development Tools: version manager, addon manager and test runner for Godot.
#[derive(Parser)]
#[command(name = "gdt", version, about)]
struct Cli {
    /// How to show progress: rich bars, plain lines, or none
    #[arg(long, global = true, value_enum, default_value = "auto")]
    progress: ProgressArg,

    /// Hide progress and informational messages
    #[arg(short, long, global = true)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show the engine requirements recorded in a project
    Inspect {
        /// A project.godot file, or a folder inside a project (default: the current folder)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Show where gdt keeps its files
    Paths,
    /// List Godot builds: installed ones, or with --remote the ones you can install
    List(ListArgs),
}

#[derive(Args)]
struct ListArgs {
    /// List what is available on GitHub instead of what is installed
    #[arg(long)]
    remote: bool,
    /// With --remote, include pre-releases (beta, rc, dev)
    #[arg(long)]
    all: bool,
    /// Ignore the cached release list and ask GitHub again
    #[arg(long)]
    refresh: bool,
    /// Print machine-readable JSON
    #[arg(long)]
    json: bool,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Error: {error:#}");
            ExitCode::from(exit_code(&error))
        }
    }
}

/// The documented exit code for an error (see docs/specs/cli.md).
fn exit_code(error: &anyhow::Error) -> u8 {
    let network = error.downcast_ref::<HttpError>().is_some()
        || matches!(
            error.downcast_ref::<ReleaseError>(),
            Some(ReleaseError::Http(_))
        );
    if network { 3 } else { 1 }
}

fn run(cli: Cli) -> Result<()> {
    let style = Style::resolve(cli.progress, cli.quiet);
    match cli.command {
        Command::Inspect { path } => inspect(&path),
        Command::Paths => paths(),
        Command::List(args) => list(&args, style),
    }
}

fn inspect(path: &std::path::Path) -> Result<()> {
    let file = find_project_file(path)?;
    let features = EngineFeatures::from_file(&file)?;
    println!("project: {}", file.display());
    match features.engine {
        Some((major, minor)) => println!("engine:  {major}.{minor}"),
        None => println!("engine:  not recorded"),
    }
    println!("dotnet:  {}", features.dotnet);
    Ok(())
}

fn paths() -> Result<()> {
    let dirs = Dirs::detect()?;
    println!("data:     {}", dirs.data_dir().display());
    println!("config:   {}", dirs.config_file().display());
    println!("versions: {}", dirs.versions_dir().display());
    println!("cache:    {}", dirs.cache_dir().display());
    println!("bin:      {}", dirs.bin_dir().display());
    Ok(())
}

fn list(args: &ListArgs, style: Style) -> Result<()> {
    let dirs = Dirs::detect()?;
    let installed = dirs.installed_builds()?;
    if args.remote {
        list_remote(args, style, &dirs, &installed)
    } else {
        list_installed(args, &installed)
    }
}

fn list_installed(args: &ListArgs, installed: &[Build]) -> Result<()> {
    if args.json {
        let versions: Vec<_> = installed
            .iter()
            .map(|b| json!({ "id": b.to_string(), "version": b.version.to_string(), "mono": b.flavor == Flavor::Dotnet }))
            .collect();
        println!("{}", json!({ "versions": versions }));
    } else if installed.is_empty() {
        println!(
            "No Godot builds are installed yet. See `gdt list --remote` for what is available."
        );
    } else {
        for build in installed {
            println!("{build}");
        }
    }
    Ok(())
}

fn list_remote(args: &ListArgs, style: Style, dirs: &Dirs, installed: &[Build]) -> Result<()> {
    let token = ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|v| !v.is_empty()));
    let http = UreqClient::new(&format!("gdt/{}", env!("CARGO_PKG_VERSION")), token);
    let cache = ResponseCache::new(dirs.cache_dir().join("api"), RELEASE_CACHE_TTL);

    let mut progress = CliProgress::new(style);
    progress.begin("Fetching Godot releases", None);
    let mut releases = fetch_releases(
        &http,
        Some(&cache),
        Source::Stable,
        args.refresh,
        &mut progress,
    )?;
    if args.all {
        releases.extend(fetch_releases(
            &http,
            Some(&cache),
            Source::PreReleases,
            args.refresh,
            &mut progress,
        )?);
    }
    progress.finish(&format!("Found {} Godot releases", releases.len()));

    releases.sort_by_key(|r| std::cmp::Reverse(r.version));
    releases.dedup_by_key(|r| r.version);

    if args.json {
        let versions: Vec<_> = releases.iter().map(|r| remote_json(r, installed)).collect();
        println!("{}", json!({ "versions": versions }));
    } else {
        for release in &releases {
            let flavors = installed_flavors(&release.version, installed);
            if flavors.is_empty() {
                println!("{}", release.version);
            } else {
                println!(
                    "{:<12} installed: {}",
                    release.version.to_string(),
                    flavors.join(", ")
                );
            }
        }
    }
    Ok(())
}

fn installed_flavors(version: &Version, installed: &[Build]) -> Vec<&'static str> {
    installed
        .iter()
        .filter(|b| b.version == *version)
        .map(|b| match b.flavor {
            Flavor::Standard => "standard",
            Flavor::Dotnet => "mono",
        })
        .collect()
}

fn remote_json(release: &Release, installed: &[Build]) -> serde_json::Value {
    json!({
        "version": release.version.to_string(),
        "tag": release.tag,
        "channel": release.version.channel.to_string(),
        "source": release.source.repo(),
        "installed": installed_flavors(&release.version, installed),
    })
}
