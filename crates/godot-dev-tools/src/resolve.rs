//! Deciding which installed Godot build a command uses (ADR 0005).
//!
//! First match wins: the `--godot` flag, the `GDT_VERSION` variable, `gdt.toml`
//! in the project, the version recorded in `project.godot`, and finally the
//! default engine. A choice that is not installed is an error, never a silent
//! fallback to another version, because opening a project with the wrong
//! version can rewrite its files.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::manifest::{MANIFEST_FILE, Manifest, ManifestError};
use crate::project::{EngineFeatures, PROJECT_FILE, ProjectError};
use crate::version::{Build, Target, Version, VersionError, VersionSpec};

/// Errors from deciding which build to use.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error(transparent)]
    Version(#[from] VersionError),
    #[error(transparent)]
    Manifest(#[from] ManifestError),
    #[error(transparent)]
    Project(#[from] ProjectError),
    /// A build was chosen but it is not installed.
    #[error(
        "Godot {spec} is required by {from}, but it is not installed; run `gdt install {spec}`"
    )]
    NotInstalled { spec: String, from: Source },
    /// Nothing asked for a version and there is no default.
    #[error(
        "no Godot build is selected; install one with `gdt install latest`, or choose one with `gdt use <version>`"
    )]
    NothingSelected,
}

/// Where the choice came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Flag,
    Environment,
    Manifest(PathBuf),
    ProjectFile(PathBuf),
    Default,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Flag => f.write_str("the --godot flag"),
            Self::Environment => f.write_str("GDT_VERSION"),
            Self::Manifest(p) => write!(f, "{}", p.display()),
            Self::ProjectFile(p) => write!(f, "{}", p.display()),
            Self::Default => f.write_str("the default engine"),
        }
    }
}

/// The build chosen, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub build: Build,
    pub source: Source,
}

/// A folder that holds a Godot project and/or its `gdt.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRoot {
    pub dir: PathBuf,
    pub manifest: Option<PathBuf>,
    pub project_file: Option<PathBuf>,
}

/// The nearest folder, starting at `start` and going up, that contains a
/// `gdt.toml` or a `project.godot`.
pub fn find_project_root(start: &Path) -> Option<ProjectRoot> {
    let start = std::path::absolute(start).ok()?;
    start.ancestors().find_map(|dir| {
        let manifest = dir.join(MANIFEST_FILE);
        let project = dir.join(PROJECT_FILE);
        let (manifest, project_file) = (
            manifest.is_file().then_some(manifest),
            project.is_file().then_some(project),
        );
        (manifest.is_some() || project_file.is_some()).then(|| ProjectRoot {
            dir: dir.to_path_buf(),
            manifest,
            project_file,
        })
    })
}

/// The folder given with Godot's `--path` flag (`--path <dir>` or `--path=<dir>`),
/// if the arguments have one. This is how a project is found when Godot is
/// started from another folder.
pub fn godot_path_arg<S: AsRef<std::ffi::OsStr>>(args: &[S]) -> Option<PathBuf> {
    let mut iter = args.iter().map(AsRef::as_ref);
    while let Some(arg) = iter.next() {
        if arg == "--path" {
            return iter.next().map(PathBuf::from);
        }
        if let Some(value) = arg.to_str().and_then(|a| a.strip_prefix("--path=")) {
            return Some(PathBuf::from(value));
        }
    }
    None
}

/// What to resolve.
pub struct Request<'a> {
    /// The `--godot` flag.
    pub flag: Option<&'a str>,
    /// The `GDT_VERSION` variable.
    pub env: Option<&'a str>,
    /// The project the command is about, if any.
    pub project: Option<&'a ProjectRoot>,
}

/// The installed build a spec stands for, if any.
pub fn pick_installed(spec: &VersionSpec, installed: &[Build]) -> Option<Build> {
    let versions: Vec<Version> = installed
        .iter()
        .filter(|b| b.flavor == spec.flavor)
        .map(|b| b.version)
        .collect();
    spec.pick(&versions).map(|version| Build {
        version,
        flavor: spec.flavor,
    })
}

/// Decides which installed build to use.
pub fn resolve(
    request: &Request<'_>,
    installed: &[Build],
    config: &Config,
) -> Result<Resolved, ResolveError> {
    let pick = |text: &str, source: Source| -> Result<Resolved, ResolveError> {
        let spec: VersionSpec = text.parse()?;
        pick_installed(&spec, installed)
            .map(|build| Resolved {
                build,
                source: source.clone(),
            })
            .ok_or(ResolveError::NotInstalled {
                spec: text.trim().to_string(),
                from: source,
            })
    };

    if let Some(text) = request.flag {
        return pick(text, Source::Flag);
    }
    if let Some(text) = request.env.filter(|t| !t.trim().is_empty()) {
        return pick(text, Source::Environment);
    }

    if let Some(project) = request.project {
        if let Some(path) = &project.manifest {
            if let Some(text) = Manifest::read(path)?.version {
                return pick(&text, Source::Manifest(path.clone()));
            }
        }
        if let Some(path) = &project.project_file {
            let features = EngineFeatures::from_file(path)?;
            if let Some((major, minor)) = features.engine {
                let flavor = if features.dotnet { "-mono" } else { "" };
                return pick(
                    &format!("{major}.{minor}{flavor}"),
                    Source::ProjectFile(path.clone()),
                );
            }
        }
    }

    match config.default_build() {
        Some(build) if installed.contains(&build) => Ok(Resolved {
            build,
            source: Source::Default,
        }),
        Some(build) => Err(ResolveError::NotInstalled {
            spec: build.to_string(),
            from: Source::Default,
        }),
        None => Err(ResolveError::NothingSelected),
    }
}

/// The builds a spec could stand for when removing one: only exact matches,
/// or a `MAJOR.MINOR` line, so that nothing is deleted by an alias.
pub fn removal_candidates(spec: &VersionSpec, installed: &[Build]) -> Option<Vec<Build>> {
    let matches = |b: &Build| {
        b.flavor == spec.flavor
            && match spec.target {
                Target::Exact(v) => b.version == v,
                Target::Line { major, minor } => {
                    b.version.channel == crate::version::Channel::Stable
                        && b.version.major == major
                        && b.version.minor == minor
                }
                _ => false,
            }
    };
    match spec.target {
        Target::Exact(_) | Target::Line { .. } => {
            Some(installed.iter().copied().filter(matches).collect())
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn build(name: &str) -> Build {
        Build::from_dir_name(name).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn installed() -> Vec<Build> {
        ["4.6.3", "4.7.1", "4.7.2", "4.7.2-mono", "4.8-beta2"]
            .map(build)
            .to_vec()
    }

    fn config(default: Option<&str>) -> Config {
        Config {
            default_engine: default.map(str::to_string),
        }
    }

    fn none() -> Request<'static> {
        Request {
            flag: None,
            env: None,
            project: None,
        }
    }

    #[test]
    fn the_default_is_used_when_nothing_else_asks() {
        let r = resolve(&none(), &installed(), &config(Some("4.7.1"))).unwrap();
        assert_eq!(r.build, build("4.7.1"));
        assert_eq!(r.source, Source::Default);
    }

    #[test]
    fn nothing_selected_is_an_error() {
        let err = resolve(&none(), &installed(), &config(None)).unwrap_err();
        assert!(matches!(err, ResolveError::NothingSelected));
    }

    #[test]
    fn a_default_that_was_uninstalled_is_reported() {
        let err = resolve(&none(), &installed(), &config(Some("4.0"))).unwrap_err();
        assert!(matches!(
            err,
            ResolveError::NotInstalled {
                from: Source::Default,
                ..
            }
        ));
    }

    #[test]
    fn the_flag_beats_the_environment_and_the_default() {
        let request = Request {
            flag: Some("4.6"),
            env: Some("4.7.1"),
            project: None,
        };
        let r = resolve(&request, &installed(), &config(Some("4.7.2"))).unwrap();
        assert_eq!(r.build, build("4.6.3"));
        assert_eq!(r.source, Source::Flag);
    }

    #[test]
    fn the_environment_beats_the_default() {
        let request = Request {
            flag: None,
            env: Some("4.7.1"),
            project: None,
        };
        let r = resolve(&request, &installed(), &config(Some("4.7.2"))).unwrap();
        assert_eq!(r.build, build("4.7.1"));
        assert_eq!(r.source, Source::Environment);
        // An empty variable is the same as no variable.
        let request = Request {
            env: Some("  "),
            ..none()
        };
        assert_eq!(
            resolve(&request, &installed(), &config(Some("4.7.2")))
                .unwrap()
                .source,
            Source::Default
        );
    }

    #[test]
    fn a_loose_spec_picks_the_newest_installed_match_and_flavor() {
        let request = Request {
            flag: Some("4.7-mono"),
            ..none()
        };
        let r = resolve(&request, &installed(), &config(None)).unwrap();
        assert_eq!(r.build, build("4.7.2-mono"));
    }

    #[test]
    fn a_choice_that_is_not_installed_never_falls_back() {
        let request = Request {
            flag: Some("4.5"),
            ..none()
        };
        let err = resolve(&request, &installed(), &config(Some("4.7.2"))).unwrap_err();
        assert!(matches!(err, ResolveError::NotInstalled { .. }));
        assert!(err.to_string().contains("gdt install 4.5"));
    }

    #[test]
    fn a_bad_spec_is_a_version_error() {
        let request = Request {
            flag: Some("banana"),
            ..none()
        };
        assert!(matches!(
            resolve(&request, &installed(), &config(None)),
            Err(ResolveError::Version(_))
        ));
    }

    #[test]
    fn the_project_decides_before_the_default() {
        let dir = scratch("resolve-project");
        fs::write(
            dir.join(PROJECT_FILE),
            "config/features=PackedStringArray(\"4.6\", \"Forward Plus\")\n",
        )
        .unwrap();
        let root = find_project_root(&dir).unwrap();
        let request = Request {
            project: Some(&root),
            ..none()
        };
        let r = resolve(&request, &installed(), &config(Some("4.7.2"))).unwrap();
        assert_eq!(r.build, build("4.6.3"));
        assert!(matches!(r.source, Source::ProjectFile(_)));

        // A .NET project wants the .NET build.
        fs::write(
            dir.join(PROJECT_FILE),
            "config/features=PackedStringArray(\"4.7\", \"C#\")\n",
        )
        .unwrap();
        let r = resolve(&request, &installed(), &config(None)).unwrap();
        assert_eq!(r.build, build("4.7.2-mono"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_manifest_beats_project_godot() {
        let dir = scratch("resolve-manifest");
        fs::write(
            dir.join(PROJECT_FILE),
            "config/features=PackedStringArray(\"4.6\")\n",
        )
        .unwrap();
        fs::write(dir.join(MANIFEST_FILE), "[godot]\nversion = \"4.7.1\"\n").unwrap();
        let root = find_project_root(&dir).unwrap();
        let request = Request {
            project: Some(&root),
            ..none()
        };
        let r = resolve(&request, &installed(), &config(None)).unwrap();
        assert_eq!(r.build, build("4.7.1"));
        assert!(matches!(r.source, Source::Manifest(_)));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_project_that_asks_for_nothing_falls_back_to_the_default() {
        let dir = scratch("resolve-silent");
        fs::write(dir.join(PROJECT_FILE), "[application]\nconfig/name=\"x\"\n").unwrap();
        let root = find_project_root(&dir).unwrap();
        let request = Request {
            project: Some(&root),
            ..none()
        };
        let r = resolve(&request, &installed(), &config(Some("4.7.2"))).unwrap();
        assert_eq!(r.source, Source::Default);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn the_path_argument_is_found() {
        let v = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            godot_path_arg(&v(&["--path", "C:/game", "-s", "x.gd"])),
            Some(PathBuf::from("C:/game"))
        );
        assert_eq!(
            godot_path_arg(&v(&["--headless", "--path=../g"])),
            Some(PathBuf::from("../g"))
        );
        assert_eq!(godot_path_arg(&v(&["-s", "x.gd"])), None);
        assert_eq!(godot_path_arg(&v(&["--path"])), None);
        // Works on raw OS arguments too, which is what the shim receives.
        let os = [
            std::ffi::OsString::from("--path"),
            std::ffi::OsString::from("p"),
        ];
        assert_eq!(godot_path_arg(&os), Some(PathBuf::from("p")));
    }

    #[test]
    fn the_project_root_is_found_from_a_subfolder() {
        let dir = scratch("root-find");
        fs::write(dir.join(MANIFEST_FILE), "").unwrap();
        let deep = dir.join("a").join("b");
        fs::create_dir_all(&deep).unwrap();
        let root = find_project_root(&deep).unwrap();
        assert_eq!(root.dir, dir);
        assert!(root.manifest.is_some() && root.project_file.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn removal_only_matches_exact_builds_or_a_line() {
        let all = installed();
        let spec = |t: &str| t.parse::<VersionSpec>().unwrap();
        assert_eq!(
            removal_candidates(&spec("4.7.1"), &all).unwrap(),
            vec![build("4.7.1")]
        );
        assert_eq!(
            removal_candidates(&spec("4.7"), &all).unwrap(),
            vec![build("4.7.1"), build("4.7.2")]
        );
        assert_eq!(
            removal_candidates(&spec("4.7.2-mono"), &all).unwrap(),
            vec![build("4.7.2-mono")]
        );
        assert_eq!(
            removal_candidates(&spec("4.8-beta2"), &all).unwrap(),
            vec![build("4.8-beta2")]
        );
        assert_eq!(removal_candidates(&spec("4.5"), &all).unwrap(), vec![]);
        assert_eq!(removal_candidates(&spec("latest"), &all), None);
        assert_eq!(removal_candidates(&spec("beta"), &all), None);
    }
}
