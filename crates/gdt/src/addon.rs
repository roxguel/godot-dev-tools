//! `gdt addon ...` and `gdt sync`: the addons of a project.
//!
//! An addon comes from the Godot Asset Store (the default: `publisher/asset`),
//! the legacy Asset Library (`library:123`) or GitHub (`github:owner/repo`).
//! Whatever the source, `add` resolves it to something exact (a commit, a
//! release), installs it, records it in `gdt.toml` and pins it in `gdt.lock`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use godot_dev_tools::guard::refuse_symlink;
use godot_dev_tools::{
    AddonEntry, AddonError, AddonStatus, ArchiveJob, EngineFeatures, Fetched, GitMode,
    GithubSource, InstallPlan, LOCK_FILE, LibraryAsset, LockFile, LockedAddon, MANIFEST_FILE,
    Manifest, Progress, SearchHit, StoreAsset, StoreRelease, addon_status, find_project_root,
    ignore_lines, install_archive, install_github, library_asset, library_search, parse_library_id,
    pick_release, remove_addon, remove_addon_dir, resolve_commit, set_addon, set_ignore_block,
    store_info, store_releases, store_search,
};
use serde_json::json;

use crate::progress::CliProgress;
use crate::{Ctx, UsageError, http_client};

#[derive(Subcommand)]
pub enum AddonCommand {
    /// Add an addon: install it, record it in gdt.toml and pin it in gdt.lock
    Add(AddArgs),
    /// Install what gdt.toml and gdt.lock say (with a source, the same as `add`)
    Install(InstallArgs),
    /// Remove an addon from the project
    #[command(visible_alias = "rm")]
    Remove(RemoveArgs),
    /// List the addons of the project and whether they are installed
    #[command(visible_alias = "ls")]
    List(ListArgs),
    /// Look for newer versions of the addons and install them
    Update(UpdateArgs),
    /// Search the Asset Store (or, with --library, the legacy Asset Library)
    Search(SearchArgs),
}

#[derive(Args)]
pub struct AddArgs {
    /// Where it comes from: publisher/asset (Asset Store), store:publisher/asset@version,
    /// library:123 (Asset Library) or github:owner/repo@ref
    source: String,
    /// The folder name under addons/ (default: the addon's folder in the archive)
    #[arg(long)]
    name: Option<String>,
    /// The folder of the archive that holds the addon (default: the only one in addons/)
    #[arg(long)]
    subdir: Option<String>,
    /// Needed to work on the project but not shipped with the game
    #[arg(short = 'D', long)]
    dev: bool,
    /// Replace addons/<name> if it already exists
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
pub struct InstallArgs {
    /// An addon to add, as in `gdt addon add` (omit it to install what the project declares)
    source: Option<String>,
    /// Fail if gdt.lock is missing an addon, instead of resolving it
    #[arg(long)]
    locked: bool,
    /// Skip the development addons
    #[arg(long)]
    no_dev: bool,
    /// Replace an addon whose files differ from the lockfile
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
pub struct RemoveArgs {
    /// The addon's name
    name: String,
    /// Remove it even if its files differ from the lockfile
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
pub struct ListArgs {
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Args)]
pub struct UpdateArgs {
    /// The addon to update (default: all of them)
    name: Option<String>,
    /// Replace an addon whose files differ from the lockfile
    #[arg(short, long)]
    force: bool,
}

#[derive(Args)]
pub struct SearchArgs {
    /// What to look for
    text: String,
    /// Search the legacy Asset Library instead of the Asset Store
    #[arg(long)]
    library: bool,
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Args)]
pub struct SyncArgs {
    /// Skip the development addons
    #[arg(long)]
    no_dev: bool,
}

/// The files of the project gdt works on.
struct Project {
    dir: PathBuf,
    manifest: PathBuf,
    lock: PathBuf,
    gitignore: PathBuf,
}

impl Project {
    fn find() -> Result<Self> {
        let root = find_project_root(Path::new("."))
            .filter(|r| r.project_file.is_some())
            .ok_or_else(|| {
                UsageError(
                    "this is not inside a Godot project (no project.godot found); run it from your project's folder"
                        .into(),
                )
            })?;
        Ok(Self {
            manifest: root.dir.join(MANIFEST_FILE),
            lock: root.dir.join(LOCK_FILE),
            gitignore: root.dir.join(".gitignore"),
            dir: root.dir,
        })
    }

    fn manifest(&self) -> Result<Manifest> {
        if !self.manifest.is_file() {
            return Ok(Manifest::default());
        }
        Ok(Manifest::read(&self.manifest)?)
    }

    /// Keeps the `.gitignore` block in step with the mode and the installed addons.
    fn update_gitignore(&self, mode: GitMode, lock: &LockFile) -> Result<()> {
        let names: Vec<&str> = lock.addons.iter().map(|a| a.name.as_str()).collect();
        let lines = ignore_lines(mode, &names);
        refuse_symlink(&self.gitignore)
            .with_context(|| format!("could not update {}", self.gitignore.display()))?;
        let current = fs::read_to_string(&self.gitignore).unwrap_or_default();
        let updated = set_ignore_block(&current, &lines);
        if updated != current {
            fs::write(&self.gitignore, updated)
                .with_context(|| format!("could not write {}", self.gitignore.display()))?;
        }
        Ok(())
    }

    /// `MAJOR.MINOR` of the engine the project asks for, to pick compatible releases.
    fn engine_version(&self) -> Option<String> {
        let features = EngineFeatures::from_file(&self.dir.join("project.godot")).ok()?;
        features
            .engine
            .map(|(major, minor)| format!("{major}.{minor}"))
    }
}

/// Where an addon comes from.
#[derive(Debug, Clone)]
enum Kind {
    Github(GithubSource),
    Store(StoreAsset),
    Library(String),
}

impl Kind {
    /// How the lockfile names the source.
    fn lock_name(&self) -> String {
        match self {
            Self::Github(source) => source.lock_name(),
            Self::Store(asset) => asset.lock_name(),
            Self::Library(id) => format!("library:{id}"),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Github(source) => source.slug(),
            Self::Store(asset) => asset.slug(),
            Self::Library(id) => format!("library:{id}"),
        }
    }
}

/// Reads what the user typed after `gdt addon add`; the second value is a
/// version asked for with `@version` on a Store asset.
fn parse_source(text: &str) -> Result<(Kind, Option<String>)> {
    let text = text.trim();
    if text.starts_with("github:")
        || text.starts_with("https://github.com/")
        || text.starts_with("http://github.com/")
        || text.starts_with("github.com/")
    {
        return Ok((Kind::Github(GithubSource::parse(text)?), None));
    }
    if text.starts_with("library:") {
        return Ok((Kind::Library(parse_library_id(text)?), None));
    }
    // `store:publisher/asset`, or a bare `publisher/asset`: the Asset Store is the default.
    let (asset, version) = StoreAsset::parse(text).map_err(|_| {
        UsageError(format!(
            "`{text}` is not an addon source. Write publisher/asset (Asset Store), \
             library:123 (Asset Library) or github:owner/repo"
        ))
    })?;
    Ok((Kind::Store(asset), version))
}

/// The source an `[addons.<name>]` entry stands for.
fn entry_kind(name: &str, entry: &AddonEntry) -> Result<Kind> {
    let given = [
        ("github", entry.github.is_some()),
        ("store", entry.store.is_some()),
        ("library", entry.library.is_some()),
    ]
    .iter()
    .filter(|(_, set)| *set)
    .map(|(key, _)| *key)
    .collect::<Vec<_>>();
    if given.len() > 1 {
        return Err(UsageError(format!(
            "[addons.{name}] names more than one source ({}); keep only one",
            given.join(", ")
        ))
        .into());
    }
    if let Some(github) = &entry.github {
        let mut source = GithubSource::parse(&format!("github:{github}"))?;
        source.git_ref = entry.git_ref.clone();
        return Ok(Kind::Github(source));
    }
    if let Some(store) = &entry.store {
        return Ok(Kind::Store(StoreAsset::parse(store)?.0));
    }
    if let Some(library) = &entry.library {
        return Ok(Kind::Library(parse_library_id(library)?));
    }
    Err(UsageError(format!(
        "[addons.{name}] needs a `github`, `store` or `library` key"
    ))
    .into())
}

/// Something exact to install: a commit, a Store release or a Library version.
enum Pinned {
    Github {
        source: GithubSource,
        sha: String,
    },
    Store {
        asset: StoreAsset,
        release: StoreRelease,
    },
    Library {
        asset: LibraryAsset,
    },
}

impl Pinned {
    fn lock_name(&self) -> String {
        match self {
            Self::Github { source, .. } => source.lock_name(),
            Self::Store { asset, .. } => asset.lock_name(),
            Self::Library { asset } => asset.lock_name(),
        }
    }

    /// What the lockfile records as the revision.
    fn rev(&self) -> String {
        match self {
            Self::Github { sha, .. } => sha.clone(),
            Self::Store { release, .. } => release.version.clone(),
            Self::Library { asset } => asset.version.clone(),
        }
    }
}

fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        &rev[..7]
    } else {
        rev
    }
}

/// A name safe to use in a cache file.
fn file_safe(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Resolves a source to something exact: the newest compatible Store release (or
/// `version`), the Library's current version, or the commit a GitHub ref points to.
fn resolve_new(
    kind: &Kind,
    version: Option<&str>,
    engine: Option<&str>,
    progress: &mut CliProgress,
) -> Result<Pinned> {
    let http = http_client();
    progress.begin(&format!("Resolving {}", kind.label()), None);
    let pinned = match kind {
        Kind::Github(source) => Pinned::Github {
            sha: resolve_commit(&http, source, source.git_ref.as_deref())?,
            source: source.clone(),
        },
        Kind::Store(asset) => {
            let info = store_info(&http, asset)?;
            if info.price_cent > 0 {
                return Err(AddonError::Paid {
                    name: asset.slug(),
                    price: format!("{}.{:02}", info.price_cent / 100, info.price_cent % 100),
                    url: info.store_url,
                }
                .into());
            }
            // A version named by the user is looked up in the whole list; otherwise
            // only the releases that work with the project's Godot.
            let compat = if version.is_some() { None } else { engine };
            let releases = store_releases(&http, asset, compat)?;
            let release = pick_release(&releases, version).ok_or_else(|| {
                AddonError::NotFound(match (version, compat) {
                    (Some(v), _) => format!("{} version {v}", asset.slug()),
                    (None, Some(g)) => {
                        format!("a release of {} that works with Godot {g}", asset.slug())
                    }
                    (None, None) => format!("a release of {}", asset.slug()),
                })
            })?;
            Pinned::Store {
                asset: asset.clone(),
                release: release.clone(),
            }
        }
        Kind::Library(id) => Pinned::Library {
            asset: library_asset(&http, id)?,
        },
    };
    progress.finish(&format!("{} is at {}", kind.label(), short(&pinned.rev())));
    Ok(pinned)
}

/// Finds again what the lockfile recorded, so it can be downloaded again.
fn pinned_from_lock(kind: &Kind, locked: &LockedAddon) -> Result<Pinned> {
    let http = http_client();
    Ok(match kind {
        Kind::Github(source) => Pinned::Github {
            source: source.clone(),
            sha: locked.rev.clone(),
        },
        Kind::Store(asset) => {
            let releases = store_releases(&http, asset, None)?;
            let release = pick_release(&releases, Some(&locked.rev)).ok_or_else(|| {
                AddonError::NotFound(format!(
                    "release {} of {} (it is no longer listed)",
                    locked.rev,
                    asset.slug()
                ))
            })?;
            Pinned::Store {
                asset: asset.clone(),
                release: release.clone(),
            }
        }
        Kind::Library(id) => {
            let asset = library_asset(&http, id)?;
            if asset.version != locked.rev {
                return Err(UsageError(format!(
                    "the Asset Library now has version {} of library:{id}, but {LOCK_FILE} has {}; \
                     run `gdt addon update {}`",
                    asset.version, locked.rev, locked.name
                ))
                .into());
            }
            Pinned::Library { asset }
        }
    })
}

/// Downloads and installs a pinned addon.
fn fetch(
    project: &Project,
    pinned: &Pinned,
    subdir: Option<&str>,
    name: Option<&str>,
    replace: bool,
    progress: &mut CliProgress,
) -> Result<Fetched> {
    let dirs = godot_dev_tools::Dirs::detect()?;
    let http = http_client();
    let fetched = match pinned {
        Pinned::Github { source, sha } => install_github(
            &http,
            &dirs,
            &project.dir,
            &InstallPlan {
                source,
                sha,
                subdir,
                name,
                replace,
            },
            progress,
        )?,
        Pinned::Store { asset, release } => install_archive(
            &http,
            &dirs,
            &project.dir,
            &ArchiveJob {
                file_name: file_safe(&format!(
                    "store-{}-{}-{}.zip",
                    asset.publisher, asset.asset, release.version
                )),
                url: release.download_url.clone(),
                label: asset.slug(),
                default_name: &asset.asset,
                subdir,
                name,
                replace,
                sha256: None,
            },
            progress,
        )?,
        Pinned::Library { asset } => {
            let default_name = file_safe(&asset.title.to_ascii_lowercase());
            install_archive(
                &http,
                &dirs,
                &project.dir,
                &ArchiveJob {
                    file_name: file_safe(&format!("library-{}-{}.zip", asset.id, asset.version)),
                    url: asset.download_url.clone(),
                    label: asset.title.clone(),
                    default_name: &default_name,
                    subdir,
                    name,
                    replace,
                    sha256: asset.download_hash.as_deref(),
                },
                progress,
            )?
        }
    };
    Ok(fetched)
}

pub fn run(command: &AddonCommand, ctx: &Ctx) -> Result<()> {
    if let AddonCommand::Search(args) = command {
        return search(args, ctx);
    }
    let project = Project::find()?;
    match command {
        AddonCommand::Add(args) => add(&project, ctx, args),
        AddonCommand::Install(args) => match &args.source {
            Some(source) => add(
                &project,
                ctx,
                &AddArgs {
                    source: source.clone(),
                    name: None,
                    subdir: None,
                    dev: false,
                    force: args.force,
                },
            ),
            None => install_all(&project, ctx, args.locked, args.no_dev, args.force),
        },
        AddonCommand::Remove(args) => remove(&project, ctx, args),
        AddonCommand::List(args) => list(&project, args),
        AddonCommand::Update(args) => update(&project, ctx, args),
        AddonCommand::Search(_) => unreachable!("handled above"),
    }
}

fn add(project: &Project, ctx: &Ctx, args: &AddArgs) -> Result<()> {
    let (kind, version) = parse_source(&args.source)?;
    let mut progress = CliProgress::new(ctx.style);
    let engine = project.engine_version();
    let pinned = resolve_new(&kind, version.as_deref(), engine.as_deref(), &mut progress)?;
    let fetched = fetch(
        project,
        &pinned,
        args.subdir.as_deref(),
        args.name.as_deref(),
        args.force,
        &mut progress,
    )?;

    let entry = AddonEntry {
        github: match &kind {
            Kind::Github(source) => Some(source.slug()),
            _ => None,
        },
        git_ref: match &kind {
            Kind::Github(source) => source.git_ref.clone(),
            _ => None,
        },
        store: match &kind {
            Kind::Store(asset) => Some(asset.slug()),
            _ => None,
        },
        library: match &kind {
            Kind::Library(id) => Some(id.clone()),
            _ => None,
        },
        // Only a version the user asked for is recorded; otherwise `update` may move on.
        version,
        subdir: Some(fetched.subdir.clone()),
        dev: args.dev,
    };
    set_addon(&project.manifest, &fetched.name, &entry)?;

    let mut lock = LockFile::load(&project.lock)?;
    lock.upsert(LockedAddon {
        name: fetched.name.clone(),
        source: pinned.lock_name(),
        rev: pinned.rev(),
        subdir: fetched.subdir.clone(),
        hash: fetched.hash.clone(),
    });
    lock.save(&project.lock)?;
    project.update_gitignore(project.manifest()?.git_mode, &lock)?;

    ctx.info(format!(
        "Added {} ({} at {}) to addons/{}",
        fetched.name,
        kind.label(),
        short(&pinned.rev()),
        fetched.name
    ));
    if project
        .dir
        .join("addons")
        .join(&fetched.name)
        .join("plugin.cfg")
        .is_file()
    {
        ctx.info(format!(
            "To use it, enable it in Project > Project Settings > Plugins (addons/{}/plugin.cfg)",
            fetched.name
        ));
    }
    Ok(())
}

fn install_all(
    project: &Project,
    ctx: &Ctx,
    locked_only: bool,
    no_dev: bool,
    force: bool,
) -> Result<()> {
    let manifest = project.manifest()?;
    if manifest.addons.is_empty() {
        ctx.info("This project declares no addons");
        return Ok(());
    }
    let mut lock = LockFile::load(&project.lock)?;
    let mut progress = CliProgress::new(ctx.style);
    let engine = project.engine_version();
    let mut changed = false;

    for (name, entry) in &manifest.addons {
        if no_dev && entry.dev {
            ctx.info(format!("Skipping {name} (development addon)"));
            continue;
        }
        let kind = entry_kind(name, entry)?;
        let existing = lock
            .get(name)
            .filter(|l| l.source == kind.lock_name())
            .cloned();

        match existing {
            Some(locked) => {
                match addon_status(&project.dir, &locked).context("could not read the addon")? {
                    AddonStatus::Installed => ctx.info(format!("{name} is already installed")),
                    status => {
                        if status == AddonStatus::Modified && !force {
                            return Err(AddonError::Modified { name: name.clone() }.into());
                        }
                        let pinned = pinned_from_lock(&kind, &locked)?;
                        let fetched = fetch(
                            project,
                            &pinned,
                            Some(&locked.subdir),
                            Some(name),
                            true,
                            &mut progress,
                        )?;
                        if fetched.hash != locked.hash {
                            remove_addon_dir(&project.dir, name)?;
                            return Err(AddonError::HashMismatch {
                                name: name.clone(),
                                expected: locked.hash.clone(),
                                actual: fetched.hash,
                            }
                            .into());
                        }
                        ctx.info(format!("Installed {name} ({})", short(&locked.rev)));
                    }
                }
            }
            None => {
                if locked_only {
                    return Err(UsageError(format!(
                        "{LOCK_FILE} has no entry for {name}; run `gdt addon install` without --locked"
                    ))
                    .into());
                }
                let pinned = resolve_new(
                    &kind,
                    entry.version.as_deref(),
                    engine.as_deref(),
                    &mut progress,
                )?;
                let fetched = fetch(
                    project,
                    &pinned,
                    entry.subdir.as_deref(),
                    Some(name),
                    force,
                    &mut progress,
                )?;
                lock.upsert(LockedAddon {
                    name: name.clone(),
                    source: pinned.lock_name(),
                    rev: pinned.rev(),
                    subdir: fetched.subdir,
                    hash: fetched.hash,
                });
                changed = true;
                ctx.info(format!("Installed {name} ({})", short(&pinned.rev())));
            }
        }
    }

    // Entries of addons that are no longer declared.
    if !locked_only {
        let stale: Vec<String> = lock
            .addons
            .iter()
            .filter(|l| !manifest.addons.contains_key(&l.name))
            .map(|l| l.name.clone())
            .collect();
        for name in stale {
            lock.remove(&name);
            changed = true;
        }
    }
    if changed {
        lock.save(&project.lock)?;
    }
    project.update_gitignore(manifest.git_mode, &lock)?;
    Ok(())
}

fn remove(project: &Project, ctx: &Ctx, args: &RemoveArgs) -> Result<()> {
    let manifest = project.manifest()?;
    let mut lock = LockFile::load(&project.lock)?;
    let name = &args.name;
    if !manifest.addons.contains_key(name) && lock.get(name).is_none() {
        return Err(UsageError(format!(
            "there is no addon called `{name}`; see `gdt addon list`"
        ))
        .into());
    }

    match lock.get(name) {
        Some(locked) => {
            if addon_status(&project.dir, locked).context("could not read the addon")?
                == AddonStatus::Modified
                && !args.force
            {
                return Err(AddonError::Modified { name: name.clone() }.into());
            }
            remove_addon_dir(&project.dir, name).context("could not remove the addon's folder")?;
        }
        None => {
            if project.dir.join("addons").join(name).exists() {
                ctx.info(format!(
                    "addons/{name} was not installed by gdt, so it was left in place"
                ));
            }
        }
    }
    lock.remove(name);
    lock.save(&project.lock)?;
    remove_addon(&project.manifest, name)?;
    project.update_gitignore(project.manifest()?.git_mode, &lock)?;
    ctx.info(format!("Removed {name}"));
    Ok(())
}

/// The source of an entry as the user would write it.
fn describe_source(entry: &AddonEntry) -> String {
    if let Some(github) = &entry.github {
        return match &entry.git_ref {
            Some(r) => format!("github:{github}@{r}"),
            None => format!("github:{github}"),
        };
    }
    if let Some(store) = &entry.store {
        return match &entry.version {
            Some(v) => format!("store:{store}@{v}"),
            None => format!("store:{store}"),
        };
    }
    if let Some(library) = &entry.library {
        return format!("library:{library}");
    }
    "(no source)".to_string()
}

fn list(project: &Project, args: &ListArgs) -> Result<()> {
    let manifest = project.manifest()?;
    let lock = LockFile::load(&project.lock)?;
    let mut rows = Vec::new();
    for (name, entry) in &manifest.addons {
        let source = describe_source(entry);
        let (rev, status) = match lock.get(name) {
            Some(locked) => (
                short(&locked.rev).to_string(),
                match addon_status(&project.dir, locked).context("could not read the addon")? {
                    AddonStatus::Installed => "installed",
                    AddonStatus::Missing => "missing",
                    AddonStatus::Modified => "modified",
                },
            ),
            None => ("-".to_string(), "not locked"),
        };
        rows.push((name.clone(), source, rev, status, entry.dev));
    }

    if args.json {
        let items: Vec<_> = rows
            .iter()
            .map(|(name, source, rev, status, dev)| {
                json!({ "name": name, "source": source, "rev": rev, "status": status, "dev": dev })
            })
            .collect();
        println!("{}", json!({ "addons": items }));
    } else if rows.is_empty() {
        println!("This project declares no addons. Add one with `gdt addon add publisher/asset`.");
    } else {
        for (name, source, rev, status, dev) in &rows {
            let dev = if *dev { "  dev" } else { "" };
            println!("{name:<20} {source:<40} {rev:<10} {status}{dev}");
        }
    }
    Ok(())
}

fn update(project: &Project, ctx: &Ctx, args: &UpdateArgs) -> Result<()> {
    let manifest = project.manifest()?;
    let names: Vec<String> = match &args.name {
        Some(name) => {
            if !manifest.addons.contains_key(name) {
                return Err(UsageError(format!("there is no addon called `{name}`")).into());
            }
            vec![name.clone()]
        }
        None => manifest.addons.keys().cloned().collect(),
    };
    let mut lock = LockFile::load(&project.lock)?;
    let mut progress = CliProgress::new(ctx.style);
    let engine = project.engine_version();

    for name in names {
        let entry = &manifest.addons[&name];
        let kind = entry_kind(&name, entry)?;
        let pinned = resolve_new(
            &kind,
            entry.version.as_deref(),
            engine.as_deref(),
            &mut progress,
        )?;
        let rev = pinned.rev();

        let current = lock.get(&name).cloned();
        if let Some(locked) = &current {
            let status = addon_status(&project.dir, locked).context("could not read the addon")?;
            if locked.rev == rev && status == AddonStatus::Installed {
                ctx.info(format!("{name} is up to date ({})", short(&rev)));
                continue;
            }
            if status == AddonStatus::Modified && !args.force {
                return Err(AddonError::Modified { name }.into());
            }
        }
        let fetched = fetch(
            project,
            &pinned,
            entry.subdir.as_deref(),
            Some(&name),
            replace_on_update(current.is_some(), args.force),
            &mut progress,
        )?;
        let from = current
            .map(|c| short(&c.rev).to_string())
            .unwrap_or_else(|| "-".into());
        lock.upsert(LockedAddon {
            name: name.clone(),
            source: pinned.lock_name(),
            rev: rev.clone(),
            subdir: fetched.subdir,
            hash: fetched.hash,
        });
        lock.save(&project.lock)?;
        ctx.info(format!("Updated {name}: {from} -> {}", short(&rev)));
    }
    project.update_gitignore(manifest.git_mode, &lock)?;
    Ok(())
}

/// Whether `update` may replace an existing `addons/<name>`. With a lock entry the
/// folder was checked against it just before; without one there is nothing to
/// compare with, so what is there may be the user's own work and only `--force`
/// replaces it.
fn replace_on_update(has_lock_entry: bool, force: bool) -> bool {
    has_lock_entry || force
}

/// `gdt addon search`: works anywhere; inside a project it prefers what works
/// with the project's Godot.
fn search(args: &SearchArgs, ctx: &Ctx) -> Result<()> {
    let http = http_client();
    let engine = Project::find().ok().and_then(|p| p.engine_version());
    let hits: Vec<SearchHit> = if args.library {
        library_search(&http, &args.text, engine.as_deref())?
    } else {
        store_search(&http, &args.text, engine.as_deref())?
    };

    let price = |hit: &SearchHit| match hit.price_cent {
        Some(0) => "free".to_string(),
        Some(_) => "paid".to_string(),
        None => String::new(),
    };
    if args.json {
        let items: Vec<_> = hits
            .iter()
            .map(|h| {
                json!({
                    "id": h.id, "name": h.name, "license": h.license,
                    "price": price(h), "detail": h.detail,
                })
            })
            .collect();
        println!("{}", json!({ "results": items }));
    } else if ctx.quiet {
        for hit in &hits {
            println!("{}", hit.id);
        }
    } else if hits.is_empty() {
        println!("Nothing found for `{}`.", args.text);
    } else {
        let last = if args.library { "VERSION" } else { "PRICE" };
        println!("{:<40} {:<32} {:<14} {last}", "ASSET", "NAME", "LICENSE");
        for hit in &hits {
            let extra = hit.detail.clone().unwrap_or_else(|| price(hit));
            println!(
                "{:<40} {:<32} {:<14} {}",
                hit.id,
                hit.name,
                hit.license.clone().unwrap_or_default(),
                extra
            );
        }
    }
    Ok(())
}

/// `gdt sync`: the engine the project asks for, then its addons.
pub fn sync(args: &SyncArgs, ctx: &Ctx) -> Result<()> {
    let project = Project::find()?;
    match crate::install(&crate::InstallArgs::for_project(), ctx) {
        Ok(()) => {}
        // A project that records no engine version has nothing to install here.
        Err(error) if error.downcast_ref::<UsageError>().is_some() => {
            ctx.info(format!("Engine: {error}"));
        }
        Err(error) => return Err(error),
    }
    install_all(&project, ctx, false, args.no_dev, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(github: bool, store: bool, library: bool) -> AddonEntry {
        AddonEntry {
            github: github.then(|| "o/r".to_string()),
            store: store.then(|| "pub/asset".to_string()),
            library: library.then(|| "123".to_string()),
            ..AddonEntry::default()
        }
    }

    #[test]
    fn an_entry_needs_exactly_one_source() {
        assert!(matches!(
            entry_kind("x", &entry(true, false, false)).unwrap(),
            Kind::Github(_)
        ));
        assert!(matches!(
            entry_kind("x", &entry(false, true, false)).unwrap(),
            Kind::Store(_)
        ));
        assert!(matches!(
            entry_kind("x", &entry(false, false, true)).unwrap(),
            Kind::Library(_)
        ));
        for (g, s, l) in [
            (false, false, false),
            (true, true, false),
            (true, true, true),
        ] {
            let error = entry_kind("x", &entry(g, s, l)).err().unwrap();
            assert!(error.downcast_ref::<UsageError>().is_some(), "{g} {s} {l}");
            assert!(error.to_string().contains("[addons.x]"));
        }
    }

    #[test]
    fn update_replaces_an_unlocked_folder_only_with_force() {
        assert!(replace_on_update(true, false), "checked against the lock");
        assert!(replace_on_update(true, true));
        assert!(!replace_on_update(false, false), "nothing to compare with");
        assert!(replace_on_update(false, true));
    }
}
