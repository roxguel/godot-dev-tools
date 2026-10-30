//! `gdt addon ...` and `gdt sync`: the addons of a project.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use godot_dev_tools::{
    AddonEntry, AddonError, AddonStatus, GitMode, GithubSource, InstallPlan, LOCK_FILE, LockFile,
    LockedAddon, MANIFEST_FILE, Manifest, Progress, addon_status, find_project_root, ignore_lines,
    install_github, remove_addon, remove_addon_dir, resolve_commit, set_addon, set_ignore_block,
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
    /// Look for newer commits of the addons and install them
    Update(UpdateArgs),
}

#[derive(Args)]
pub struct AddArgs {
    /// Where it comes from: github:owner/repo or github:owner/repo@ref (a tag, branch or commit)
    source: String,
    /// The folder name under addons/ (default: the addon's folder in the repository)
    #[arg(long)]
    name: Option<String>,
    /// The folder of the repository that holds the addon (default: the only one in addons/)
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
        let current = fs::read_to_string(&self.gitignore).unwrap_or_default();
        let updated = set_ignore_block(&current, &lines);
        if updated != current {
            fs::write(&self.gitignore, updated)
                .with_context(|| format!("could not write {}", self.gitignore.display()))?;
        }
        Ok(())
    }
}

/// `github:owner/repo` or a GitHub URL; a bare `publisher/asset` is the Asset Store.
fn github_source(text: &str) -> Result<GithubSource> {
    let text = text.trim();
    let is_github = text.starts_with("github:")
        || text.starts_with("https://github.com/")
        || text.starts_with("http://github.com/");
    if !is_github {
        return Err(UsageError(format!(
            "`{text}` is not a GitHub source. For now addons come from GitHub: write \
             `github:owner/repo` or `github:owner/repo@ref` (the Asset Store and the Asset \
             Library are not supported yet)"
        ))
        .into());
    }
    Ok(GithubSource::parse(text)?)
}

fn short(rev: &str) -> &str {
    rev.get(..7).unwrap_or(rev)
}

pub fn run(command: &AddonCommand, ctx: &Ctx) -> Result<()> {
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
    }
}

fn add(project: &Project, ctx: &Ctx, args: &AddArgs) -> Result<()> {
    let dirs = godot_dev_tools::Dirs::detect()?;
    let source = github_source(&args.source)?;
    let http = http_client();
    let mut progress = CliProgress::new(ctx.style);

    progress.begin(&format!("Resolving {}", source.slug()), None);
    let sha = resolve_commit(&http, &source, source.git_ref.as_deref())?;
    progress.finish(&format!("{} is at {}", source.slug(), short(&sha)));

    let fetched = install_github(
        &http,
        &dirs,
        &project.dir,
        &InstallPlan {
            source: &source,
            sha: &sha,
            subdir: args.subdir.as_deref(),
            name: args.name.as_deref(),
            replace: args.force,
        },
        &mut progress,
    )?;

    set_addon(
        &project.manifest,
        &fetched.name,
        &AddonEntry {
            github: Some(source.slug()),
            git_ref: source.git_ref.clone(),
            subdir: Some(fetched.subdir.clone()),
            dev: args.dev,
        },
    )?;
    let mut lock = LockFile::load(&project.lock)?;
    lock.upsert(LockedAddon {
        name: fetched.name.clone(),
        source: source.lock_name(),
        rev: sha.clone(),
        subdir: fetched.subdir.clone(),
        hash: fetched.hash.clone(),
    });
    lock.save(&project.lock)?;
    project.update_gitignore(project.manifest()?.git_mode, &lock)?;

    ctx.info(format!(
        "Added {} ({} at {}) to addons/{}",
        fetched.name,
        source.slug(),
        short(&sha),
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

/// The source and reference an `[addons.<name>]` entry stands for.
fn entry_source(name: &str, entry: &AddonEntry) -> Result<GithubSource> {
    let github = entry.github.as_deref().ok_or_else(|| {
        UsageError(format!(
            "[addons.{name}] has no `github` key; only GitHub sources are supported so far"
        ))
    })?;
    let mut source = GithubSource::parse(&format!("github:{github}"))?;
    source.git_ref = entry.git_ref.clone();
    Ok(source)
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
    let dirs = godot_dev_tools::Dirs::detect()?;
    let http = http_client();
    let mut lock = LockFile::load(&project.lock)?;
    let mut progress = CliProgress::new(ctx.style);
    let mut changed = false;

    for (name, entry) in &manifest.addons {
        if no_dev && entry.dev {
            ctx.info(format!("Skipping {name} (development addon)"));
            continue;
        }
        let source = entry_source(name, entry)?;
        let existing = lock
            .get(name)
            .filter(|l| l.source == source.lock_name())
            .cloned();

        match existing {
            Some(locked) => {
                match addon_status(&project.dir, &locked).context("could not read the addon")? {
                    AddonStatus::Installed => ctx.info(format!("{name} is already installed")),
                    status => {
                        if status == AddonStatus::Modified && !force {
                            return Err(AddonError::Modified { name: name.clone() }.into());
                        }
                        let fetched = install_github(
                            &http,
                            &dirs,
                            &project.dir,
                            &InstallPlan {
                                source: &source,
                                sha: &locked.rev,
                                subdir: Some(&locked.subdir),
                                name: Some(name),
                                replace: true,
                            },
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
                progress.begin(&format!("Resolving {}", source.slug()), None);
                let sha = resolve_commit(&http, &source, source.git_ref.as_deref())?;
                progress.finish(&format!("{} is at {}", source.slug(), short(&sha)));
                let fetched = install_github(
                    &http,
                    &dirs,
                    &project.dir,
                    &InstallPlan {
                        source: &source,
                        sha: &sha,
                        subdir: entry.subdir.as_deref(),
                        name: Some(name),
                        replace: force,
                    },
                    &mut progress,
                )?;
                lock.upsert(LockedAddon {
                    name: name.clone(),
                    source: source.lock_name(),
                    rev: sha.clone(),
                    subdir: fetched.subdir,
                    hash: fetched.hash,
                });
                changed = true;
                ctx.info(format!("Installed {name} ({})", short(&sha)));
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

fn list(project: &Project, args: &ListArgs) -> Result<()> {
    let manifest = project.manifest()?;
    let lock = LockFile::load(&project.lock)?;
    let mut rows = Vec::new();
    for (name, entry) in &manifest.addons {
        let source = entry
            .github
            .as_ref()
            .map(|g| match &entry.git_ref {
                Some(r) => format!("github:{g}@{r}"),
                None => format!("github:{g}"),
            })
            .unwrap_or_else(|| "(no source)".to_string());
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
        println!(
            "This project declares no addons. Add one with `gdt addon add github:owner/repo`."
        );
    } else {
        for (name, source, rev, status, dev) in &rows {
            let dev = if *dev { "  dev" } else { "" };
            println!("{name:<20} {source:<36} {rev:<8} {status}{dev}");
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
    let dirs = godot_dev_tools::Dirs::detect()?;
    let http = http_client();
    let mut lock = LockFile::load(&project.lock)?;
    let mut progress = CliProgress::new(ctx.style);

    for name in names {
        let entry = &manifest.addons[&name];
        let source = entry_source(&name, entry)?;
        progress.begin(&format!("Resolving {}", source.slug()), None);
        let sha = resolve_commit(&http, &source, source.git_ref.as_deref())?;
        progress.finish(&format!("{} is at {}", source.slug(), short(&sha)));

        let current = lock.get(&name).cloned();
        if let Some(locked) = &current {
            let status = addon_status(&project.dir, locked).context("could not read the addon")?;
            if locked.rev == sha && status == AddonStatus::Installed {
                ctx.info(format!("{name} is up to date ({})", short(&sha)));
                continue;
            }
            if status == AddonStatus::Modified && !args.force {
                return Err(AddonError::Modified { name }.into());
            }
        }
        let fetched = install_github(
            &http,
            &dirs,
            &project.dir,
            &InstallPlan {
                source: &source,
                sha: &sha,
                subdir: entry.subdir.as_deref(),
                name: Some(&name),
                replace: true,
            },
            &mut progress,
        )?;
        let from = current
            .map(|c| short(&c.rev).to_string())
            .unwrap_or_else(|| "-".into());
        lock.upsert(LockedAddon {
            name: name.clone(),
            source: source.lock_name(),
            rev: sha.clone(),
            subdir: fetched.subdir,
            hash: fetched.hash,
        });
        lock.save(&project.lock)?;
        ctx.info(format!("Updated {name}: {from} -> {}", short(&sha)));
    }
    project.update_gitignore(manifest.git_mode, &lock)?;
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
