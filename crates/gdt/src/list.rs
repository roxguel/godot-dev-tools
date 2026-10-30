//! `gdt list`: installed builds, or with `--remote` what can be installed.
//!
//! Each line has a gutter, the version, and tags:
//!
//! ```text
//! * 4.7.2      latest, standard (default, in use here), mono
//! * 4.7.1
//!   4.8-rc1    rc
//! ```
//!
//! `*` means installed, `in use here` is the build that applies in the current
//! folder, `default` is the global default, and `latest`, `rc`, `beta` and `dev`
//! name the newest release of that channel (the aliases `gdt install` accepts).

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use clap::{Args, ValueEnum};
use console::style;
use godot_dev_tools::{
    Build, ChannelFilter, ChannelKind, Config, Dirs, Flavor, Progress, Release, ResolveError,
    Resolved, Source, Version, fetch_releases,
};
use serde_json::json;

use crate::progress::CliProgress;
use crate::{Ctx, http_client, release_cache, select};

/// The values of `--channel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum ChannelArg {
    Stable,
    Rc,
    Beta,
    Dev,
    /// Every pre-release channel: rc, beta and dev
    Pre,
}

#[derive(Args)]
pub struct ListArgs {
    /// List what is available on GitHub instead of what is installed
    #[arg(short, long)]
    remote: bool,
    /// Every channel: stable, rc, beta and dev (installed builds are always all shown)
    #[arg(short, long)]
    all: bool,
    /// Only these channels, separated by commas (for example `rc,beta`)
    #[arg(short, long, value_enum, value_delimiter = ',', value_name = "CHANNEL")]
    channel: Vec<ChannelArg>,
    /// Only the .NET builds
    #[arg(short, long)]
    mono: bool,
    /// Ignore the cached release list and ask GitHub again
    #[arg(long)]
    refresh: bool,
    /// Print machine-readable JSON
    #[arg(short, long)]
    json: bool,
}

impl ListArgs {
    /// The channels to show. Remote lists are stable-only unless asked otherwise;
    /// the installed list shows everything.
    fn filter(&self) -> ChannelFilter {
        if self.channel.is_empty() {
            return if self.remote && !self.all {
                ChannelFilter::STABLE
            } else {
                ChannelFilter::ALL
            };
        }
        let mut filter = ChannelFilter::default();
        for channel in &self.channel {
            match channel {
                ChannelArg::Stable => filter.stable = true,
                ChannelArg::Rc => filter.rc = true,
                ChannelArg::Beta => filter.beta = true,
                ChannelArg::Dev => filter.dev = true,
                ChannelArg::Pre => {
                    filter.rc = true;
                    filter.beta = true;
                    filter.dev = true;
                }
            }
        }
        filter
    }
}

/// Which builds are the default and which one applies in the current folder.
struct Marks {
    default: Option<Build>,
    in_use: Option<Resolved>,
}

impl Marks {
    fn detect(ctx: &Ctx, dirs: &Dirs) -> Result<Self> {
        let default = Config::load(&dirs.config_file())?.default_build();
        let in_use = match select::resolve_for(ctx, dirs, Path::new(".")) {
            Ok(resolved) => Some(resolved),
            Err(error) => {
                // A project that asks for a build that is not installed is worth saying.
                if matches!(
                    error.downcast_ref::<ResolveError>(),
                    Some(ResolveError::NotInstalled { .. })
                ) {
                    ctx.info(format!("Note: {error}"));
                }
                None
            }
        };
        Ok(Self { default, in_use })
    }

    fn is_default(&self, build: &Build) -> bool {
        self.default == Some(*build)
    }

    fn is_in_use(&self, build: &Build) -> bool {
        self.in_use.as_ref().is_some_and(|r| r.build == *build)
    }

    fn describe_in_use(&self, ctx: &Ctx) {
        if let Some(resolved) = &self.in_use {
            ctx.info(format!(
                "In use here: Godot {} (from {})",
                resolved.build, resolved.source
            ));
        }
    }

    fn json(&self) -> serde_json::Value {
        json!({
            "default": self.default.map(|b| b.to_string()),
            "in_use": self.in_use.as_ref().map(|r| r.build.to_string()),
            "in_use_from": self.in_use.as_ref().map(|r| r.source.to_string()),
        })
    }
}

/// What a tag means, which decides its color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    InUse,
    Default,
    Alias,
    Plain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Tag {
    text: String,
    kind: Kind,
}

impl Tag {
    fn new(text: impl Into<String>, kind: Kind) -> Self {
        Self {
            text: text.into(),
            kind,
        }
    }
}

/// One line of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    installed: bool,
    in_use: bool,
    name: String,
    tags: Vec<Tag>,
}

impl Row {
    /// The line as text, with colors when the terminal shows them.
    fn render(&self) -> String {
        let gutter = if self.installed {
            style("*").green().to_string()
        } else {
            " ".to_string()
        };
        let padded = format!("{:<12}", self.name);
        let name = if self.in_use {
            style(padded).green().bold().to_string()
        } else if self.installed {
            padded
        } else {
            style(padded).dim().to_string()
        };
        let tags: Vec<String> = self
            .tags
            .iter()
            .map(|t| match t.kind {
                Kind::InUse => style(&t.text).green().to_string(),
                Kind::Default => style(&t.text).cyan().to_string(),
                Kind::Alias => style(&t.text).yellow().to_string(),
                Kind::Plain => t.text.clone(),
            })
            .collect();
        format!("{gutter} {name} {}", tags.join(", "))
            .trim_end()
            .to_string()
    }

    /// The line without any styling, for tests.
    #[cfg(test)]
    fn plain(&self) -> String {
        let tags: Vec<&str> = self.tags.iter().map(|t| t.text.as_str()).collect();
        let gutter = if self.installed { '*' } else { ' ' };
        format!("{gutter} {:<12} {}", self.name, tags.join(", "))
            .trim_end()
            .to_string()
    }
}

fn flavor_name(flavor: Flavor) -> &'static str {
    match flavor {
        Flavor::Standard => "standard",
        Flavor::Dotnet => "mono",
    }
}

/// The newest release of each channel, named by the alias that picks it.
fn alias_tags(releases: &[Release]) -> HashMap<Version, Vec<&'static str>> {
    let newest = |kind: Option<ChannelKind>| {
        releases
            .iter()
            .map(|r| r.version)
            .filter(|v| v.channel.kind() == kind)
            .max()
    };
    let mut map: HashMap<Version, Vec<&'static str>> = HashMap::new();
    for (kind, alias) in [
        (None, "latest"),
        (Some(ChannelKind::Rc), "rc"),
        (Some(ChannelKind::Beta), "beta"),
        (Some(ChannelKind::Dev), "dev"),
    ] {
        if let Some(version) = newest(kind) {
            map.entry(version).or_default().push(alias);
        }
    }
    map
}

/// Marks for a build: where it applies, and whether it is the default.
fn build_tags(build: &Build, marks: &Marks) -> Vec<Tag> {
    let mut tags = Vec::new();
    if marks.is_in_use(build) {
        tags.push(Tag::new("in use here", Kind::InUse));
    }
    if marks.is_default(build) {
        tags.push(Tag::new("default", Kind::Default));
    }
    tags
}

fn installed_rows(builds: &[Build], marks: &Marks) -> Vec<Row> {
    builds
        .iter()
        .map(|b| Row {
            installed: true,
            in_use: marks.is_in_use(b),
            name: b.to_string(),
            tags: build_tags(b, marks),
        })
        .collect()
}

fn remote_rows(
    releases: &[Release],
    aliases: &HashMap<Version, Vec<&'static str>>,
    installed: &[Build],
    marks: &Marks,
) -> Vec<Row> {
    releases
        .iter()
        .map(|release| {
            let here: Vec<&Build> = installed
                .iter()
                .filter(|b| b.version == release.version)
                .collect();
            let mut tags: Vec<Tag> = aliases
                .get(&release.version)
                .into_iter()
                .flatten()
                .map(|a| Tag::new(*a, Kind::Alias))
                .collect();
            for build in &here {
                let marks_here = build_tags(build, marks);
                let text = if marks_here.is_empty() {
                    flavor_name(build.flavor).to_string()
                } else {
                    let labels: Vec<&str> = marks_here.iter().map(|t| t.text.as_str()).collect();
                    format!("{} ({})", flavor_name(build.flavor), labels.join(", "))
                };
                let kind = if marks.is_in_use(build) {
                    Kind::InUse
                } else if marks.is_default(build) {
                    Kind::Default
                } else {
                    Kind::Plain
                };
                tags.push(Tag::new(text, kind));
            }
            Row {
                installed: !here.is_empty(),
                in_use: here.iter().any(|b| marks.is_in_use(b)),
                name: release.version.to_string(),
                tags,
            }
        })
        .collect()
}

pub fn list(args: &ListArgs, ctx: &Ctx) -> Result<()> {
    let dirs = Dirs::detect()?;
    let filter = args.filter();
    let installed: Vec<Build> = dirs
        .installed_builds()?
        .into_iter()
        .filter(|b| !args.mono || b.flavor == Flavor::Dotnet)
        .collect();
    let marks = if ctx.quiet {
        Marks {
            default: None,
            in_use: None,
        }
    } else {
        Marks::detect(ctx, &dirs)?
    };

    if args.remote {
        list_remote(args, ctx, &dirs, filter, &installed, &marks)
    } else {
        let shown: Vec<Build> = installed
            .into_iter()
            .filter(|b| filter.matches(&b.version.channel))
            .collect();
        list_installed(args, ctx, &shown, &marks)
    }
}

fn list_installed(args: &ListArgs, ctx: &Ctx, shown: &[Build], marks: &Marks) -> Result<()> {
    if ctx.quiet {
        for build in shown {
            println!("{build}");
        }
        return Ok(());
    }
    if args.json {
        let versions: Vec<_> = shown
            .iter()
            .map(|b| {
                json!({
                    "id": b.to_string(),
                    "version": b.version.to_string(),
                    "mono": b.flavor == Flavor::Dotnet,
                    "default": marks.is_default(b),
                    "in_use": marks.is_in_use(b),
                })
            })
            .collect();
        let mut out = marks.json();
        out["versions"] = versions.into();
        println!("{out}");
    } else if shown.is_empty() {
        println!(
            "No Godot builds are installed yet. See `gdt list --remote` for what is available."
        );
    } else {
        for row in installed_rows(shown, marks) {
            println!("{}", row.render());
        }
        marks.describe_in_use(ctx);
    }
    Ok(())
}

fn list_remote(
    args: &ListArgs,
    ctx: &Ctx,
    dirs: &Dirs,
    filter: ChannelFilter,
    installed: &[Build],
    marks: &Marks,
) -> Result<()> {
    let http = http_client();
    let cache = release_cache(dirs);

    let mut progress = CliProgress::new(ctx.style);
    progress.begin("Fetching Godot releases", None);
    let mut releases = Vec::new();
    if filter.stable {
        releases.extend(fetch_releases(
            &http,
            Some(&cache),
            Source::Stable,
            args.refresh,
            &mut progress,
        )?);
    }
    if filter.includes_pre_releases() {
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
    // The aliases name the newest of each channel among everything fetched,
    // before the list is narrowed to the channels asked for.
    let aliases = alias_tags(&releases);
    releases.retain(|r| filter.matches(&r.version.channel));

    if ctx.quiet {
        for release in &releases {
            println!("{}", release.version);
        }
        return Ok(());
    }
    if args.json {
        let versions: Vec<_> = releases
            .iter()
            .map(|r| {
                let names: Vec<&str> = installed
                    .iter()
                    .filter(|b| b.version == r.version)
                    .map(|b| flavor_name(b.flavor))
                    .collect();
                json!({
                    "version": r.version.to_string(),
                    "tag": r.tag,
                    "channel": r.version.channel.to_string(),
                    "source": r.source.repo(),
                    "aliases": aliases.get(&r.version).cloned().unwrap_or_default(),
                    "installed": names,
                })
            })
            .collect();
        let mut out = marks.json();
        out["versions"] = versions.into();
        println!("{out}");
    } else {
        for row in remote_rows(&releases, &aliases, installed, marks) {
            println!("{}", row.render());
        }
        marks.describe_in_use(ctx);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(name: &str) -> Build {
        Build::from_dir_name(name).unwrap()
    }

    fn release(tag: &str, source: Source) -> Release {
        Release {
            version: Version::from_tag(tag).unwrap(),
            tag: tag.to_string(),
            source,
            assets: vec![],
        }
    }

    fn marks(default: &str, in_use: &str) -> Marks {
        Marks {
            default: Some(build(default)),
            in_use: Some(Resolved {
                build: build(in_use),
                source: godot_dev_tools::resolve::Source::Default,
            }),
        }
    }

    fn args(remote: bool, all: bool, channel: &[ChannelArg]) -> ListArgs {
        ListArgs {
            remote,
            all,
            channel: channel.to_vec(),
            mono: false,
            refresh: false,
            json: false,
        }
    }

    #[test]
    fn the_channels_shown_depend_on_the_flags() {
        assert_eq!(args(false, false, &[]).filter(), ChannelFilter::ALL);
        assert_eq!(args(true, false, &[]).filter(), ChannelFilter::STABLE);
        assert_eq!(args(true, true, &[]).filter(), ChannelFilter::ALL);

        let rc_beta = args(true, false, &[ChannelArg::Rc, ChannelArg::Beta]).filter();
        assert!(rc_beta.rc && rc_beta.beta && !rc_beta.stable && !rc_beta.dev);

        let pre = args(true, false, &[ChannelArg::Pre]).filter();
        assert!(pre.rc && pre.beta && pre.dev && !pre.stable);
        // An explicit channel list wins over --all.
        let only_stable = args(true, true, &[ChannelArg::Stable]).filter();
        assert_eq!(only_stable, ChannelFilter::STABLE);
    }

    #[test]
    fn installed_rows_carry_the_marks() {
        let builds = [build("4.6.3"), build("4.7.2"), build("4.7.2-mono")];
        let rows = installed_rows(&builds, &marks("4.7.2", "4.6.3"));
        let lines: Vec<String> = rows.iter().map(Row::plain).collect();
        assert_eq!(lines[0], "* 4.6.3        in use here");
        assert_eq!(lines[1], "* 4.7.2        default");
        assert_eq!(lines[2], "* 4.7.2-mono");
    }

    #[test]
    fn a_build_that_is_both_shows_both() {
        let rows = installed_rows(&[build("4.7.2")], &marks("4.7.2", "4.7.2"));
        assert_eq!(rows[0].plain(), "* 4.7.2        in use here, default");
    }

    #[test]
    fn aliases_name_the_newest_of_each_channel() {
        let releases = [
            release("4.7.2-stable", Source::Stable),
            release("4.7.1-stable", Source::Stable),
            release("4.8-rc1", Source::PreReleases),
            release("4.8-beta2", Source::PreReleases),
            release("4.8-beta1", Source::PreReleases),
            release("4.9-dev1", Source::PreReleases),
        ];
        let aliases = alias_tags(&releases);
        let of = |tag: &str| aliases.get(&Version::from_tag(tag).unwrap()).cloned();
        assert_eq!(of("4.7.2-stable"), Some(vec!["latest"]));
        assert_eq!(of("4.8-rc1"), Some(vec!["rc"]));
        assert_eq!(of("4.8-beta2"), Some(vec!["beta"]));
        assert_eq!(of("4.9-dev1"), Some(vec!["dev"]));
        assert_eq!(of("4.7.1-stable"), None);
        assert_eq!(of("4.8-beta1"), None);
    }

    #[test]
    fn remote_rows_separate_installed_from_available() {
        let releases = [
            release("4.8-rc1", Source::PreReleases),
            release("4.7.2-stable", Source::Stable),
            release("4.7.1-stable", Source::Stable),
        ];
        let aliases = alias_tags(&releases);
        let installed = [build("4.7.2"), build("4.7.2-mono")];
        let rows = remote_rows(&releases, &aliases, &installed, &marks("4.7.2", "4.7.2"));
        let lines: Vec<String> = rows.iter().map(Row::plain).collect();
        assert_eq!(lines[0], "  4.8-rc1      rc");
        assert_eq!(
            lines[1],
            "* 4.7.2        latest, standard (in use here, default), mono"
        );
        assert_eq!(lines[2], "  4.7.1");
        assert!(rows[1].installed && rows[1].in_use && !rows[0].installed);
    }

    #[test]
    fn the_row_text_has_no_escape_codes_without_color() {
        console::set_colors_enabled(false);
        let rows = installed_rows(&[build("4.7.2")], &marks("4.7.2", "4.7.2"));
        assert_eq!(rows[0].render(), rows[0].plain());
    }
}
