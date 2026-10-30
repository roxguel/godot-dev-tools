mod addon;
mod list;
mod open;
mod path_env;
mod progress;
mod run;
mod select;
mod shim;
mod test;
mod verbose;

use std::env;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use anyhow::Result;
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand};
use godot_dev_tools::{
    AddonError, Arch, Build, Config, Dirs, EngineError, EngineFeatures, Flavor, HttpClient,
    HttpError, InstallRequest, Platform, Progress, ReleaseError, ResolveError, ResponseCache,
    Source, TestError, UreqClient, Version, VersionError, VersionSpec, fetch_releases,
    find_project_file, install_build,
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

    /// Show diagnostic details on stderr (-vv for network and TLS details too)
    #[arg(short, long, global = true, action = ArgAction::Count)]
    verbose: u8,

    /// Use this installed Godot build for this command, whatever the project asks for
    #[arg(short, long, global = true, value_name = "SPEC")]
    godot: Option<String>,

    /// When to use colors: auto, always or never
    #[arg(long, global = true, value_enum, default_value = "auto")]
    color: ColorArg,

    #[command(subcommand)]
    command: Option<Command>,
}

/// The values of `--color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ColorArg {
    /// Colors when writing to a terminal (and `NO_COLOR` is not set)
    Auto,
    Always,
    Never,
}

#[derive(Subcommand)]
enum Command {
    #[command(flatten)]
    Shortcut(EngineShortcut),
    /// Manage Godot builds
    Engine {
        #[command(subcommand)]
        command: EngineCommand,
    },
    /// Show the engine requirements recorded in a project
    Inspect {
        /// A project.godot file, or a folder inside a project (default: the current folder)
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    /// Run Godot with the right version: a GDScript file, or any Godot arguments
    Run(run::RunArgs),
    /// Open a project in the editor with the version it asks for
    Open(run::OpenArgs),
    /// Manage the addons of a project
    Addon {
        #[command(subcommand)]
        command: addon::AddonCommand,
    },
    /// Make a fresh clone ready: the engine the project asks for, then its addons
    Sync(addon::SyncArgs),
    /// Run the project's tests headless (GUT, gdUnit4 or a script)
    Test(test::TestArgs),
    /// Manage the `godot` shim, which makes a plain `godot` follow gdt
    Shim {
        #[command(subcommand)]
        command: shim::ShimCommand,
    },
    /// Show where gdt keeps its files, or open one of them
    Paths {
        /// Open this folder or file in the file manager
        #[arg(short, long, value_enum, value_name = "WHAT")]
        open: Option<open::Place>,
    },
    /// There is no top-level `add`
    #[command(hide = true)]
    Add {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        rest: Vec<String>,
    },
}

/// The engine commands that also work without the `engine` prefix.
#[derive(Subcommand)]
enum EngineShortcut {
    /// Download, verify and install a Godot build
    Install(InstallArgs),
    /// List Godot builds: installed ones, or with --remote the ones you can install
    #[command(visible_alias = "ls")]
    List(list::ListArgs),
    /// Show the Godot build in use here
    Current(select::CurrentArgs),
    /// Remove an installed Godot build
    Uninstall(select::UninstallArgs),
    /// Choose the default Godot build, or show it
    #[command(name = "use")]
    UseBuild(select::UseArgs),
    /// Record the Godot build a project uses, in its gdt.toml
    #[command(name = "pin")]
    Pin(select::PinArgs),
    /// Show which Godot executable would run here
    Which(select::WhichArgs),
}

#[derive(Subcommand)]
enum EngineCommand {
    #[command(flatten)]
    Shortcut(EngineShortcut),
}

#[derive(Args)]
struct InstallArgs {
    /// What to install: 4, 4.7, 4.7.2, latest, beta, rc, dev, pre, 4.8-rc1 (add -mono for .NET).
    /// Without it, installs what the current project asks for
    spec: Option<String>,
    /// Install the .NET build (same as the -mono suffix)
    #[arg(short, long)]
    mono: bool,
    /// Ignore the cached release list and ask GitHub again
    #[arg(long)]
    refresh: bool,
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

impl InstallArgs {
    /// What `gdt sync` does for the engine: install what the project asks for.
    fn for_project() -> Self {
        Self {
            spec: None,
            mono: false,
            refresh: false,
            json: false,
        }
    }
}

/// Settings shared by every command.
struct Ctx {
    style: Style,
    quiet: bool,
    /// The `--godot` flag.
    godot: Option<String>,
}

impl Ctx {
    /// An informational message, hidden by --quiet.
    fn info(&self, message: impl fmt::Display) {
        if !self.quiet {
            eprintln!("{message}");
        }
    }
}

/// A mistake in how the command was written.
#[derive(Debug)]
struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

/// Ends the process with this exit code and no message: what a program run by
/// `gdt run` returned, passed on unchanged (a full `i32`: a Windows crash code
/// such as 0xC0000005 must not be squeezed into a byte).
#[derive(Debug)]
struct ExitWith(i32);

impl fmt::Display for ExitWith {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "exited with code {}", self.0)
    }
}

impl std::error::Error for ExitWith {}

fn main() -> ExitCode {
    // Started as `godot` (the shim), gdt does nothing but run the right Godot. This
    // is checked first, before argument parsing, so that every argument reaches Godot.
    if shim::invoked_as_shim() {
        std::process::exit(shim::run());
    }

    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if let Some(ExitWith(code)) = error.downcast_ref::<ExitWith>() {
                std::process::exit(*code);
            }
            eprintln!("Error: {error:#}");
            ExitCode::from(exit_code(&error))
        }
    }
}

/// The documented exit code for an error (see docs/specs/cli.md).
fn exit_code(error: &anyhow::Error) -> u8 {
    if error.downcast_ref::<UsageError>().is_some()
        || error.downcast_ref::<VersionError>().is_some()
    {
        return 2;
    }
    if let Some(error) = error.downcast_ref::<ResolveError>() {
        return match error {
            ResolveError::Version(_) => 2,
            ResolveError::NotInstalled { .. } | ResolveError::NothingSelected => 4,
            _ => 1,
        };
    }
    if let Some(error) = error.downcast_ref::<AddonError>() {
        return match error {
            AddonError::BadSource(_) | AddonError::BadName(_) => 2,
            AddonError::Http(_) => 3,
            AddonError::NotFound(_) => 4,
            AddonError::HashMismatch { .. } => 5,
            AddonError::Paid { .. } => 6,
            _ => 1,
        };
    }
    if error.downcast_ref::<TestError>().is_some() {
        return 2;
    }
    if error.downcast_ref::<HttpError>().is_some() {
        return 3;
    }
    if let Some(error) = error.downcast_ref::<ReleaseError>() {
        return if matches!(error, ReleaseError::Http(_)) {
            3
        } else {
            1
        };
    }
    match error.downcast_ref::<EngineError>() {
        Some(EngineError::Http(_)) => 3,
        Some(
            EngineError::VersionNotFound(_)
            | EngineError::NotInstalled(_)
            | EngineError::NoBuild(_),
        ) => 4,
        Some(
            EngineError::ChecksumMismatch { .. }
            | EngineError::NoChecksum(_)
            | EngineError::NoChecksumFile,
        ) => 5,
        _ => 1,
    }
}

fn run(cli: Cli) -> Result<()> {
    verbose::init(cli.verbose);
    match cli.color {
        ColorArg::Auto => {}
        ColorArg::Always => console::set_colors_enabled(true),
        ColorArg::Never => console::set_colors_enabled(false),
    }
    // Redrawn bars would be torn apart by the log lines, so -v uses plain lines.
    let arg = if cli.verbose > 0 && cli.progress == ProgressArg::Auto {
        ProgressArg::Plain
    } else {
        cli.progress
    };
    let ctx = Ctx {
        style: Style::resolve(arg, cli.quiet),
        quiet: cli.quiet,
        godot: cli.godot,
    };

    match cli.command {
        None => {
            Cli::command().print_help()?;
            println!();
            Ok(())
        }
        Some(Command::Shortcut(command)) => engine(command, &ctx),
        Some(Command::Engine {
            command: EngineCommand::Shortcut(command),
        }) => engine(command, &ctx),
        Some(Command::Addon { command }) => addon::run(&command, &ctx),
        Some(Command::Sync(args)) => addon::sync(&args, &ctx),
        Some(Command::Test(args)) => test::test(&args, &ctx),
        Some(Command::Shim { command }) => shim::command(&command, &ctx),
        Some(Command::Run(args)) => run::run(&args, &ctx),
        Some(Command::Open(args)) => run::open(&args, &ctx),
        Some(Command::Inspect { path }) => inspect(&path),
        Some(Command::Paths { open }) => paths(open, &ctx),
        Some(Command::Add { .. }) => Err(UsageError(
            "there is no `gdt add`; use `gdt addon add <source>` to add an addon, \
             or `gdt install <version>` to install a Godot build"
                .into(),
        )
        .into()),
    }
}

fn engine(command: EngineShortcut, ctx: &Ctx) -> Result<()> {
    match command {
        EngineShortcut::Install(args) => install(&args, ctx),
        EngineShortcut::List(args) => list::list(&args, ctx),
        EngineShortcut::Current(args) => select::current(&args, ctx),
        EngineShortcut::Uninstall(args) => select::uninstall(&args, ctx),
        EngineShortcut::UseBuild(args) => select::use_build(&args, ctx),
        EngineShortcut::Pin(args) => select::pin_build(&args, ctx),
        EngineShortcut::Which(args) => select::which(&args, ctx),
    }
}

fn inspect(path: &Path) -> Result<()> {
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

fn paths(open_what: Option<open::Place>, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    if let Some(place) = open_what {
        let shown = open::open_place(place, &dirs)?;
        ctx.info(format!("Opened {}", shown.display()));
        return Ok(());
    }
    println!("data:     {}", dirs.data_dir().display());
    println!("config:   {}", dirs.config_file().display());
    println!("versions: {}", dirs.versions_dir().display());
    println!("cache:    {}", dirs.cache_dir().display());
    println!("bin:      {}", dirs.bin_dir().display());
    Ok(())
}

/// The HTTP client, with the GitHub token from the environment if there is one.
fn http_client() -> UreqClient {
    let token = ["GITHUB_TOKEN", "GH_TOKEN"]
        .iter()
        .find_map(|name| env::var(name).ok().filter(|v| !v.is_empty()));
    log::debug!(
        "GitHub token: {}",
        if token.is_some() { "set" } else { "not set" }
    );
    UreqClient::new(&format!("gdt/{}", env!("CARGO_PKG_VERSION")), token)
}

fn release_cache(dirs: &Dirs) -> ResponseCache {
    ResponseCache::new(dirs.api_cache_dir(), RELEASE_CACHE_TTL)
}

/// The version a project asks for in its `project.godot`, as a spec.
fn project_spec(ctx: &Ctx) -> Result<VersionSpec> {
    let file = find_project_file(Path::new("."))?;
    let features = EngineFeatures::from_file(&file)?;
    let Some((major, minor)) = features.engine else {
        return Err(UsageError(format!(
            "{} does not record an engine version; say which one to install, for example `gdt install latest`",
            file.display()
        ))
        .into());
    };
    ctx.info(format!("{} asks for Godot {major}.{minor}", file.display()));
    let flavor = if features.dotnet { "-mono" } else { "" };
    Ok(format!("{major}.{minor}{flavor}").parse()?)
}

fn install(args: &InstallArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let mut spec = match &args.spec {
        Some(text) => text.parse::<VersionSpec>()?,
        None => project_spec(ctx)?,
    };
    if args.mono {
        spec.flavor = Flavor::Dotnet;
    }
    let wanted = args
        .spec
        .clone()
        .unwrap_or_else(|| "the project's version".into());
    log::debug!("resolving {wanted}: {spec:?}");

    let http = http_client();
    let cache = release_cache(&dirs);
    let mut progress = CliProgress::new(ctx.style);

    progress.begin("Fetching Godot releases", None);
    let source = if spec.needs_pre_releases() {
        Source::PreReleases
    } else {
        Source::Stable
    };
    let releases = fetch_releases(&http, Some(&cache), source, args.refresh, &mut progress)?;
    progress.finish(&format!("Found {} Godot releases", releases.len()));

    let versions: Vec<Version> = releases.iter().map(|r| r.version).collect();
    let picked = spec
        .pick(&versions)
        .ok_or_else(|| EngineError::VersionNotFound(wanted.clone()))?;
    log::debug!("{wanted} resolves to {picked}");
    let release = releases
        .iter()
        .find(|r| r.version == picked)
        .ok_or(EngineError::VersionNotFound(wanted))?;

    let installed = install_build(
        &http as &dyn HttpClient,
        &dirs,
        &InstallRequest {
            release,
            flavor: spec.flavor,
            platform: Platform::current(),
            arch: Arch::current(),
        },
        &mut progress,
    )?;
    if installed.already_installed {
        ctx.info(format!("Godot {} is already installed", installed.build));
    }

    let made_default = make_default_if_none(&dirs, &installed.build, ctx)?;
    if args.json {
        println!(
            "{}",
            json!({
                "id": installed.build.to_string(),
                "version": installed.build.version.to_string(),
                "path": installed.dir,
                "executable": installed.executable,
                "already_installed": installed.already_installed,
                "made_default": made_default,
            })
        );
    }
    Ok(())
}

/// The first build installed becomes the default, so the tool works at once.
fn make_default_if_none(dirs: &Dirs, build: &Build, ctx: &Ctx) -> Result<bool> {
    let path = dirs.config_file();
    let mut config = Config::load(&path)?;
    if config.default_build().is_some() {
        return Ok(false);
    }
    config.default_engine = Some(build.to_string());
    config.save(&path)?;
    ctx.info(format!("Godot {build} is now the default engine"));
    Ok(true)
}
