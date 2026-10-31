//! Installing addons from GitHub (see `docs/specs/addons.md`).
//!
//! A source is resolved to a full commit first, so that what is installed is
//! exactly what the lockfile records. The repository is downloaded as a zip of
//! that commit, unpacked next to `addons/` (so the final move stays on one
//! volume), and the folder that holds the addon is moved to `addons/<name>`.

use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::engine::{EngineError, Links, extract_zip};
use crate::guard::refuse_symlink;
use crate::http::{HttpClient, HttpError};
use crate::lock::{LockedAddon, hash_dir};
use crate::names::{addon_name_problem, normalize_subdir};
use crate::progress::Progress;
use crate::storage::Dirs;

/// Errors from resolving or installing an addon.
#[derive(Debug, thiserror::Error)]
pub enum AddonError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("{0}")]
    BadSource(String),
    #[error(
        "{0} was not found; check its name and version (a private GitHub repository needs GH_TOKEN)"
    )]
    NotFound(String),
    #[error("GitHub sent an answer that could not be read: {0}")]
    BadReply(String),
    #[error("{0}")]
    NoAddonFound(String),
    #[error("the repository has several addons ({}); choose one with --subdir", .0.join(", "))]
    AmbiguousSubdir(Vec<String>),
    #[error("the repository has no folder `{0}`")]
    SubdirMissing(String),
    #[error("{name} is a paid asset ({price}); gdt cannot buy it. Get it at {url}")]
    Paid {
        name: String,
        price: String,
        url: String,
    },
    #[error("`{name}` is not a usable addon name: {why}")]
    BadName { name: String, why: &'static str },
    #[error(
        "addons/{name} has files that gdt did not install or that were changed; use --force to overwrite or remove it anyway"
    )]
    Modified { name: String },
    #[error("addons/{name} does not match the lockfile (expected {expected}, got {actual})")]
    HashMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    #[error(transparent)]
    Archive(#[from] EngineError),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

fn io_err(context: impl Into<String>) -> impl FnOnce(io::Error) -> AddonError {
    let context = context.into();
    move |source| AddonError::Io { context, source }
}

/// A repository on GitHub, with the reference asked for, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubSource {
    pub owner: String,
    pub repo: String,
    pub git_ref: Option<String>,
}

impl GithubSource {
    /// `owner/repo`.
    pub fn slug(&self) -> String {
        format!("{}/{}", self.owner, self.repo)
    }

    /// How the lockfile names this source.
    pub fn lock_name(&self) -> String {
        format!("github:{}", self.slug())
    }

    /// Reads `owner/repo`, optionally with `@ref`, with or without a `github:`
    /// prefix, or a `https://github.com/owner/repo` URL (also `.git` and
    /// `/tree/<ref>`).
    pub fn parse(text: &str) -> Result<Self, AddonError> {
        let bad = || {
            AddonError::BadSource(format!(
                "`{text}` is not a GitHub source; write `github:owner/repo` or `github:owner/repo@ref`"
            ))
        };
        let text = text.trim();
        let (path, mut git_ref) = if let Some(rest) = text
            .strip_prefix("https://github.com/")
            .or_else(|| text.strip_prefix("http://github.com/"))
            .or_else(|| text.strip_prefix("github.com/"))
        {
            let mut parts = rest.trim_end_matches('/').split('/');
            let owner = parts.next().ok_or_else(bad)?;
            let repo = parts.next().ok_or_else(bad)?.trim_end_matches(".git");
            let git_ref = match (parts.next(), parts.next()) {
                (Some("tree"), Some(r)) => Some(r.to_string()),
                _ => None,
            };
            (format!("{owner}/{repo}"), git_ref)
        } else {
            (
                text.strip_prefix("github:").unwrap_or(text).to_string(),
                None,
            )
        };

        let path = match path.rsplit_once('@') {
            Some((slug, r)) if !r.is_empty() => {
                git_ref = Some(r.to_string());
                slug.to_string()
            }
            _ => path,
        };
        let (owner, repo) = path.split_once('/').ok_or_else(bad)?;
        let valid = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !valid(owner) || !valid(repo) {
            return Err(bad());
        }
        Ok(Self {
            owner: owner.to_string(),
            repo: repo.to_string(),
            git_ref,
        })
    }
}

/// The full commit that `git_ref` (a tag, branch or commit; the default branch
/// if `None`) points to.
pub fn resolve_commit(
    http: &dyn HttpClient,
    source: &GithubSource,
    git_ref: Option<&str>,
) -> Result<String, AddonError> {
    let reference = git_ref.unwrap_or("HEAD");
    let url = format!(
        "https://api.github.com/repos/{}/{}/commits/{reference}",
        source.owner, source.repo
    );
    let response = match http.get(&url) {
        Ok(response) => response,
        Err(HttpError::Status {
            status: 404 | 422, ..
        }) => {
            return Err(AddonError::NotFound(format!(
                "{} at `{reference}`",
                source.slug()
            )));
        }
        Err(e) => return Err(e.into()),
    };
    let json: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|e| AddonError::BadReply(e.to_string()))?;
    let sha = json
        .get("sha")
        .and_then(serde_json::Value::as_str)
        .filter(|s| s.len() == 40 && s.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| AddonError::BadReply("no commit id in the answer".into()))?;
    Ok(sha.to_ascii_lowercase())
}

/// What an install produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fetched {
    pub name: String,
    /// The folder of the repository that was copied (`.` for the whole repository).
    pub subdir: String,
    pub hash: String,
}

/// What to install and where.
pub struct InstallPlan<'a> {
    pub source: &'a GithubSource,
    /// The full commit to install.
    pub sha: &'a str,
    /// The folder inside the repository, or `None` to find it.
    pub subdir: Option<&'a str>,
    /// The folder name under `addons/`, or `None` to take it from the repository.
    pub name: Option<&'a str>,
    /// Replace `addons/<name>` if it exists.
    pub replace: bool,
}

/// One archive to download and unpack into `addons/`.
pub struct ArchiveJob<'a> {
    /// A name for the cached file, unique to this exact release.
    pub file_name: String,
    pub url: String,
    /// What to call it in messages, for example `bitwes/Gut`.
    pub label: String,
    /// The folder name when nothing else says (repository or asset name).
    pub default_name: &'a str,
    /// The folder inside the archive, or `None` to find it.
    pub subdir: Option<&'a str>,
    /// The folder name under `addons/`, or `None` to take it from the archive.
    pub name: Option<&'a str>,
    /// Replace `addons/<name>` if it exists.
    pub replace: bool,
    /// The SHA-256 the publisher gave for the archive, to check it against.
    pub sha256: Option<&'a str>,
}

fn sha256_file(path: &Path) -> io::Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

/// The name an archive has in the cache. `file_name` has odd characters replaced,
/// so two different sources can end up with the same one; the start of a digest of
/// the URL tells them apart.
fn cache_file_name(job: &ArchiveJob<'_>) -> String {
    use sha2::{Digest, Sha256};
    let digest: String = Sha256::digest(job.url.as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect();
    format!("{digest}-{}", job.file_name)
}

fn cached_archive(
    http: &dyn HttpClient,
    dirs: &Dirs,
    job: &ArchiveJob<'_>,
    progress: &mut dyn Progress,
) -> Result<PathBuf, AddonError> {
    // The `download_url` of a store asset is chosen by its publisher.
    if !job.url.starts_with("https://") {
        return Err(AddonError::BadSource(format!(
            "{} would be downloaded over plain HTTP ({}); gdt only downloads over HTTPS",
            job.label, job.url
        )));
    }
    let cache = dirs.cache_dir();
    let cache_name = cache_file_name(job);
    let archive = cache.join(&cache_name);
    let expected = job.sha256.filter(|h| !h.is_empty());
    if archive.is_file() {
        // A published checksum is checked again on every use, as for engine
        // builds, so a file that changed in the cache is not trusted.
        let intact = match expected {
            Some(expected) => {
                sha256_file(&archive).is_ok_and(|actual| actual.eq_ignore_ascii_case(expected))
            }
            None => true,
        };
        if intact {
            log::debug!("using the cached archive {}", archive.display());
            return Ok(archive);
        }
        log::warn!(
            "the cached archive {} does not match its checksum; downloading it again",
            archive.display()
        );
        let _ = fs::remove_file(&archive);
    }
    fs::create_dir_all(&cache).map_err(io_err("could not create the cache folder"))?;
    let part = cache.join(format!("{cache_name}.part"));
    let file = File::create(&part).map_err(io_err("could not create the download file"))?;
    let mut writer = BufWriter::new(file);

    progress.begin(&format!("Downloading {}", job.label), None);
    log::debug!("downloading {}", job.url);
    let result = http.download(&job.url, &mut writer, &mut |_total, read| {
        progress.advance(read)
    });
    let flushed = writer.flush();
    drop(writer);
    if let Err(error) = result {
        let _ = fs::remove_file(&part);
        return Err(match error {
            HttpError::Status { status: 404, .. } => AddonError::NotFound(job.label.clone()),
            other => other.into(),
        });
    }
    if let Err(error) = flushed {
        let _ = fs::remove_file(&part);
        return Err(io_err("could not write the download")(error));
    }
    if let Some(expected) = expected {
        let actual = sha256_file(&part).map_err(io_err("could not read the download"))?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = fs::remove_file(&part);
            return Err(AddonError::HashMismatch {
                name: job.label.clone(),
                expected: expected.to_string(),
                actual,
            });
        }
    }
    progress.finish(&format!("Downloaded {}", job.label));
    fs::rename(&part, &archive).map_err(io_err("could not store the download"))?;
    Ok(archive)
}

/// The only folder inside `dir`, if it holds exactly one entry and it is a folder
/// (a GitHub zip wraps everything in one), otherwise `dir` itself.
fn single_root(dir: &Path) -> PathBuf {
    let entries: Vec<PathBuf> = fs::read_dir(dir)
        .map(|d| d.filter_map(Result::ok).map(|e| e.path()).collect())
        .unwrap_or_default();
    match entries.as_slice() {
        [only] if only.is_dir() => only.clone(),
        _ => dir.to_path_buf(),
    }
}

/// Finds the folder that holds the addon in an unpacked repository.
fn find_subdir(root: &Path) -> Result<String, AddonError> {
    let addons = root.join("addons");
    if addons.is_dir() {
        let mut found: Vec<String> = fs::read_dir(&addons)
            .map_err(io_err("could not read the repository"))?
            .filter_map(Result::ok)
            .filter(|e| e.path().is_dir())
            .map(|e| format!("addons/{}", e.file_name().to_string_lossy()))
            .collect();
        found.sort();
        return match found.len() {
            0 => Err(AddonError::NoAddonFound(
                "the repository has an addons folder but nothing in it".into(),
            )),
            1 => Ok(found.remove(0)),
            _ => Err(AddonError::AmbiguousSubdir(found)),
        };
    }
    if root.join("plugin.cfg").is_file() {
        return Ok(".".to_string());
    }
    Err(AddonError::NoAddonFound(
        "the repository has no addons folder and no plugin.cfg at its top; say where the addon is with --subdir"
            .into(),
    ))
}

/// Checks that `name` can be the folder `addons/<name>` (see [`addon_name_problem`]).
pub fn validate_name(name: &str) -> Result<(), AddonError> {
    match addon_name_problem(name) {
        None => Ok(()),
        Some(why) => Err(AddonError::BadName {
            name: name.to_string(),
            why,
        }),
    }
}

/// Downloads the commit and installs the addon into `<project>/addons/<name>`.
pub fn install_github(
    http: &dyn HttpClient,
    dirs: &Dirs,
    project_dir: &Path,
    plan: &InstallPlan<'_>,
    progress: &mut dyn Progress,
) -> Result<Fetched, AddonError> {
    install_archive(
        http,
        dirs,
        project_dir,
        &ArchiveJob {
            file_name: format!(
                "github-{}-{}-{}.zip",
                plan.source.owner, plan.source.repo, plan.sha
            ),
            url: format!(
                "https://api.github.com/repos/{}/{}/zipball/{}",
                plan.source.owner, plan.source.repo, plan.sha
            ),
            label: plan.source.slug(),
            default_name: &plan.source.repo,
            subdir: plan.subdir,
            name: plan.name,
            replace: plan.replace,
            sha256: None,
        },
        progress,
    )
}

/// Downloads an archive and installs the addon in it into `<project>/addons/<name>`.
pub fn install_archive(
    http: &dyn HttpClient,
    dirs: &Dirs,
    project_dir: &Path,
    job: &ArchiveJob<'_>,
    progress: &mut dyn Progress,
) -> Result<Fetched, AddonError> {
    let archive = cached_archive(http, dirs, job, progress)?;
    let addons_dir = project_dir.join("addons");
    refuse_symlink(&addons_dir).map_err(io_err("could not use the addons folder"))?;
    fs::create_dir_all(&addons_dir).map_err(io_err("could not create the addons folder"))?;

    let staging = addons_dir.join(format!(".gdt-tmp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    let result = place(&archive, &addons_dir, &staging, job);
    let _ = fs::remove_dir_all(&staging);
    result
}

fn place(
    archive: &Path,
    addons_dir: &Path,
    staging: &Path,
    job: &ArchiveJob<'_>,
) -> Result<Fetched, AddonError> {
    extract_zip(archive, staging, Links::Reject)?;
    let root = single_root(staging);

    let subdir = match job.subdir {
        Some(given) => normalize_subdir(given).map_err(|why| {
            AddonError::BadSource(format!("the folder `{given}` cannot be used: {why}"))
        })?,
        None => find_subdir(&root)?,
    };
    let source_dir = if subdir == "." {
        root.clone()
    } else {
        root.join(&subdir)
    };
    if !source_dir.is_dir() {
        return Err(AddonError::SubdirMissing(subdir));
    }
    // Whatever the path said, what is moved must live inside the unpacked archive.
    let inside = match (source_dir.canonicalize(), root.canonicalize()) {
        (Ok(source), Ok(root)) => source.starts_with(root),
        _ => false,
    };
    if !inside {
        return Err(AddonError::SubdirMissing(subdir));
    }

    let name = match job.name {
        Some(name) => name.to_string(),
        None if subdir == "." => job.default_name.to_string(),
        None => subdir.rsplit('/').next().unwrap_or(&subdir).to_string(),
    };
    validate_name(&name)?;

    let target = addons_dir.join(&name);
    refuse_symlink(&target).map_err(io_err(format!("could not use addons/{name}")))?;
    if target.exists() {
        if !job.replace {
            return Err(AddonError::Modified { name });
        }
        fs::remove_dir_all(&target).map_err(io_err(format!("could not replace addons/{name}")))?;
    }
    fs::rename(&source_dir, &target).map_err(io_err(format!("could not install addons/{name}")))?;
    let hash = hash_dir(&target).map_err(io_err(format!("could not read addons/{name}")))?;
    Ok(Fetched { name, subdir, hash })
}

/// `<project>/addons/<name>`, refusing a name that is not one plain folder name,
/// so that nothing outside `addons/` can be read or deleted through it.
fn addon_dir(project_dir: &Path, name: &str) -> io::Result<PathBuf> {
    match addon_name_problem(name) {
        None => {
            let addons = project_dir.join("addons");
            let dir = addons.join(name);
            // A link here would make the reads and deletes leave the project.
            refuse_symlink(&addons)?;
            refuse_symlink(&dir)?;
            Ok(dir)
        }
        Some(why) => Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("`{name}` is not a usable addon name: {why}"),
        )),
    }
}

/// How an installed addon compares with the lockfile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddonStatus {
    /// `addons/<name>` does not exist.
    Missing,
    /// It matches the lockfile.
    Installed,
    /// It exists but its files differ from the lockfile.
    Modified,
}

/// Compares `<project>/addons/<name>` with its lockfile entry.
pub fn addon_status(project_dir: &Path, locked: &LockedAddon) -> io::Result<AddonStatus> {
    let dir = addon_dir(project_dir, &locked.name)?;
    if !dir.is_dir() {
        return Ok(AddonStatus::Missing);
    }
    Ok(if hash_dir(&dir)? == locked.hash {
        AddonStatus::Installed
    } else {
        AddonStatus::Modified
    })
}

/// Deletes `<project>/addons/<name>`; nothing happens if it is not there.
pub fn remove_addon_dir(project_dir: &Path, name: &str) -> io::Result<()> {
    let dir = addon_dir(project_dir, name)?;
    match fs::remove_dir_all(&dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use crate::progress::NoProgress;
    use crate::storage::Platform;
    use std::cell::RefCell;
    use std::io::Cursor;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data.as_bytes()).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    const SHA: &str = "0123456789abcdef0123456789abcdef01234567";

    struct Fake {
        archive: Vec<u8>,
        downloads: RefCell<u32>,
        urls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(entries: &[(&str, &str)]) -> Self {
            Self {
                archive: zip_bytes(entries),
                downloads: RefCell::new(0),
                urls: RefCell::new(vec![]),
            }
        }
    }

    impl HttpClient for Fake {
        fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
            self.urls.borrow_mut().push(url.to_string());
            if url.contains("/commits/missing") {
                return Err(HttpError::Status {
                    url: url.into(),
                    status: 404,
                });
            }
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: format!("{{\"sha\":\"{}\"}}", SHA.to_uppercase()).into_bytes(),
            })
        }

        fn download(
            &self,
            url: &str,
            sink: &mut dyn Write,
            on_chunk: &mut dyn FnMut(Option<u64>, u64),
        ) -> Result<(), HttpError> {
            self.urls.borrow_mut().push(url.to_string());
            *self.downloads.borrow_mut() += 1;
            sink.write_all(&self.archive).unwrap();
            on_chunk(None, self.archive.len() as u64);
            Ok(())
        }
    }

    fn dirs(root: &Path) -> Dirs {
        Dirs::resolve(Platform::Linux, |k| {
            (k == "GDT_HOME").then(|| root.join("home").into_os_string())
        })
        .unwrap()
    }

    fn source() -> GithubSource {
        GithubSource::parse("github:bitwes/Gut").unwrap()
    }

    fn plan<'a>(
        source: &'a GithubSource,
        subdir: Option<&'a str>,
        name: Option<&'a str>,
        replace: bool,
    ) -> InstallPlan<'a> {
        InstallPlan {
            source,
            sha: SHA,
            subdir,
            name,
            replace,
        }
    }

    #[test]
    fn sources_are_parsed_in_every_spelling() {
        let p = |t: &str| GithubSource::parse(t).unwrap();
        let plain = p("github:bitwes/Gut");
        assert_eq!(
            (
                plain.owner.as_str(),
                plain.repo.as_str(),
                plain.git_ref.clone()
            ),
            ("bitwes", "Gut", None)
        );
        assert_eq!(p("bitwes/Gut@v9.4.0").git_ref.as_deref(), Some("v9.4.0"));
        assert_eq!(
            p("github:bitwes/Gut@v9.4.0").lock_name(),
            "github:bitwes/Gut"
        );
        assert_eq!(p("https://github.com/bitwes/Gut").slug(), "bitwes/Gut");
        assert_eq!(p("https://github.com/bitwes/Gut.git").repo, "Gut");
        assert_eq!(p("github.com/bitwes/Gut@v9").git_ref.as_deref(), Some("v9"));
        assert_eq!(
            p("https://github.com/bitwes/Gut/tree/main")
                .git_ref
                .as_deref(),
            Some("main")
        );
        assert_eq!(
            p("github:o/r@feature/x").git_ref.as_deref(),
            Some("feature/x")
        );
    }

    #[test]
    fn things_that_are_not_github_sources_are_rejected() {
        for bad in [
            "",
            "gut",
            "github:",
            "github:owner",
            "a/b/c d",
            "github:/repo",
            "github:owner/",
        ] {
            assert!(
                matches!(GithubSource::parse(bad), Err(AddonError::BadSource(_))),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_reference_becomes_a_full_lowercase_commit() {
        let fake = Fake::new(&[]);
        let sha = resolve_commit(&fake, &source(), Some("v9.4.0")).unwrap();
        assert_eq!(sha, SHA);
        assert!(fake.urls.borrow()[0].ends_with("/repos/bitwes/Gut/commits/v9.4.0"));
        resolve_commit(&fake, &source(), None).unwrap();
        assert!(fake.urls.borrow()[1].ends_with("/commits/HEAD"));
        let missing = resolve_commit(&fake, &source(), Some("missing"));
        assert!(matches!(missing, Err(AddonError::NotFound(_))));
    }

    #[test]
    fn the_addon_folder_is_found_and_installed_under_its_name() {
        let root = scratch("addon-install");
        let project = root.join("project");
        let fake = Fake::new(&[
            ("bitwes-Gut-0123456/README.md", "readme"),
            ("bitwes-Gut-0123456/addons/gut/plugin.cfg", "[plugin]"),
            ("bitwes-Gut-0123456/addons/gut/gut.gd", "extends Node"),
        ]);
        let src = source();
        let fetched = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, false),
            &mut NoProgress,
        )
        .unwrap();
        assert_eq!(fetched.name, "gut");
        assert_eq!(fetched.subdir, "addons/gut");
        assert!(fetched.hash.starts_with("sha256:"));
        assert_eq!(
            fs::read_to_string(project.join("addons/gut/gut.gd")).unwrap(),
            "extends Node"
        );
        assert!(
            !project.join("addons/gut/README.md").exists(),
            "only the addon folder is installed"
        );
        let leftovers: Vec<_> = fs::read_dir(project.join("addons"))
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(leftovers.len(), 1, "no staging folder is left behind");
        assert!(
            fake.urls.borrow()[0].ends_with("/zipball/0123456789abcdef0123456789abcdef01234567")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn the_name_and_subdir_can_be_chosen() {
        let root = scratch("addon-name");
        let project = root.join("project");
        let fake = Fake::new(&[
            ("o-r-abc/addons/a/plugin.cfg", "a"),
            ("o-r-abc/addons/b/plugin.cfg", "b"),
        ]);
        let src = source();
        let ambiguous = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, false),
            &mut NoProgress,
        );
        assert!(
            matches!(ambiguous, Err(AddonError::AmbiguousSubdir(ref l)) if l == &["addons/a", "addons/b"])
        );

        let fetched = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, Some("addons/b"), Some("renamed"), false),
            &mut NoProgress,
        )
        .unwrap();
        assert_eq!(fetched.name, "renamed");
        assert_eq!(
            fs::read_to_string(project.join("addons/renamed/plugin.cfg")).unwrap(),
            "b"
        );
        assert_eq!(
            *fake.downloads.borrow(),
            1,
            "the archive of a commit is downloaded once"
        );

        let missing = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, Some("nope"), None, false),
            &mut NoProgress,
        );
        assert!(matches!(missing, Err(AddonError::SubdirMissing(_))));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_repository_that_is_the_addon_is_installed_whole() {
        let root = scratch("addon-whole");
        let project = root.join("project");
        let fake = Fake::new(&[
            ("o-Dialogue-abc/plugin.cfg", "[plugin]"),
            ("o-Dialogue-abc/main.gd", "x"),
        ]);
        let src = GithubSource::parse("github:o/Dialogue").unwrap();
        let fetched = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, false),
            &mut NoProgress,
        )
        .unwrap();
        assert_eq!(
            (fetched.name.as_str(), fetched.subdir.as_str()),
            ("Dialogue", ".")
        );
        assert!(project.join("addons/Dialogue/main.gd").is_file());

        let nothing = Fake::new(&[("o-r-abc/README.md", "x")]);
        let other = scratch("addon-whole-none");
        let err = install_github(
            &nothing,
            &dirs(&other),
            &other.join("p"),
            &plan(&src, None, None, false),
            &mut NoProgress,
        );
        assert!(matches!(err, Err(AddonError::NoAddonFound(_))));
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(other);
    }

    #[test]
    fn an_existing_folder_is_only_replaced_when_allowed() {
        let root = scratch("addon-replace");
        let project = root.join("project");
        fs::create_dir_all(project.join("addons/gut")).unwrap();
        fs::write(project.join("addons/gut/mine.gd"), "my change").unwrap();
        let fake = Fake::new(&[("o-r-abc/addons/gut/plugin.cfg", "new")]);
        let src = source();

        let refused = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, false),
            &mut NoProgress,
        );
        assert!(matches!(refused, Err(AddonError::Modified { .. })));
        assert!(
            project.join("addons/gut/mine.gd").is_file(),
            "nothing was touched"
        );

        install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, true),
            &mut NoProgress,
        )
        .unwrap();
        assert!(!project.join("addons/gut/mine.gd").exists());
        assert!(project.join("addons/gut/plugin.cfg").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn unsafe_names_are_refused() {
        for bad in ["", ".hidden", "a/b", "..", "a\\b", "con", "x."] {
            assert!(
                matches!(validate_name(bad), Err(AddonError::BadName { .. })),
                "{bad:?}"
            );
        }
        for ok in ["gut", "Godot-Jolt", "a.b"] {
            assert!(validate_name(ok).is_ok(), "{ok:?}");
        }
    }

    #[test]
    fn a_name_that_escapes_addons_never_touches_the_disk() {
        let root = scratch("addon-escape");
        let project = root.join("project");
        fs::create_dir_all(project.join("addons/gut")).unwrap();
        fs::write(project.join("keep.txt"), "mine").unwrap();
        for bad in ["..", "../..", "../x", "a/b", "C:\\x"] {
            let error = remove_addon_dir(&project, bad).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "{bad:?}");
            let locked = LockedAddon {
                name: bad.into(),
                source: "github:o/r".into(),
                rev: SHA.into(),
                subdir: ".".into(),
                hash: "sha256:0".into(),
            };
            assert!(addon_status(&project, &locked).is_err(), "{bad:?}");
        }
        assert!(project.join("keep.txt").is_file(), "the project is intact");
        assert!(project.join("addons/gut").is_dir());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_folder_outside_the_archive_is_never_moved() {
        let root = scratch("addon-subdir");
        let project = root.join("project");
        let outside = root.join("precious");
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("data.txt"), "mine").unwrap();
        let fake = Fake::new(&[("o-r-abc/addons/gut/plugin.cfg", "x")]);
        let src = source();
        let absolute = outside.to_string_lossy().to_string();
        for bad in [absolute.as_str(), "..", "../..", "addons/../..", "/etc"] {
            let result = install_github(
                &fake,
                &dirs(&root),
                &project,
                &plan(&src, Some(bad), Some("x"), false),
                &mut NoProgress,
            );
            assert!(result.is_err(), "{bad:?} was accepted");
            assert!(outside.join("data.txt").is_file(), "{bad:?} moved a folder");
            assert!(!project.join("addons/x").exists(), "{bad:?} installed");
        }
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn a_linked_addons_folder_is_never_followed() {
        let root = scratch("addon-linked");
        let project = root.join("project");
        let outside = root.join("precious");
        fs::create_dir_all(project.join("x")).unwrap();
        fs::create_dir_all(outside.join("gut")).unwrap();
        fs::write(outside.join("gut/data.txt"), "mine").unwrap();
        std::os::unix::fs::symlink(&outside, project.join("addons")).unwrap();

        let fake = Fake::new(&[("o-r-abc/addons/gut/plugin.cfg", "x")]);
        let src = source();
        let result = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, true),
            &mut NoProgress,
        );
        assert!(result.is_err());
        assert!(remove_addon_dir(&project, "gut").is_err());
        let locked = LockedAddon {
            name: "gut".into(),
            source: "github:o/r".into(),
            rev: SHA.into(),
            subdir: ".".into(),
            hash: "sha256:0".into(),
        };
        assert!(addon_status(&project, &locked).is_err());
        assert!(
            outside.join("gut/data.txt").is_file(),
            "nothing was deleted"
        );
        assert!(
            !outside.join("gut/plugin.cfg").exists(),
            "nothing was written"
        );
        let _ = fs::remove_dir_all(root);
    }

    fn job<'a>(url: &str, file_name: &str, sha256: Option<&'a str>) -> ArchiveJob<'a> {
        ArchiveJob {
            file_name: file_name.into(),
            url: url.into(),
            label: "o/r".into(),
            default_name: "gut",
            subdir: None,
            name: None,
            replace: true,
            sha256,
        }
    }

    #[test]
    fn plain_http_downloads_are_refused() {
        let root = scratch("addon-http");
        let fake = Fake::new(&[("o-r-abc/plugin.cfg", "x")]);
        let result = install_archive(
            &fake,
            &dirs(&root),
            &root.join("project"),
            &job("http://example.com/a.zip", "a.zip", None),
            &mut NoProgress,
        );
        assert!(
            matches!(result, Err(AddonError::BadSource(_))),
            "{result:?}"
        );
        assert_eq!(*fake.downloads.borrow(), 0, "nothing was requested");
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn sources_that_differ_only_in_odd_characters_do_not_share_a_cache_file() {
        // `file_safe` turns both into `store-a_b-c-1.zip`.
        let a = job("https://x/a", "store-a_b-c-1.zip", None);
        let b = job("https://x/b", "store-a_b-c-1.zip", None);
        assert_ne!(cache_file_name(&a), cache_file_name(&b));
        assert_eq!(cache_file_name(&a), cache_file_name(&a));
    }

    #[test]
    fn a_cached_archive_is_checked_again_when_a_checksum_is_known() {
        let root = scratch("addon-cache-check");
        let project = root.join("project");
        let fake = Fake::new(&[("o-r-abc/plugin.cfg", "x")]);
        let archive = root.join("reference.zip");
        fs::write(&archive, &fake.archive).unwrap();
        let sha = sha256_file(&archive).unwrap();
        let job = job("https://x/a.zip", "a.zip", Some(&sha));

        install_archive(&fake, &dirs(&root), &project, &job, &mut NoProgress).unwrap();
        assert_eq!(*fake.downloads.borrow(), 1);
        install_archive(&fake, &dirs(&root), &project, &job, &mut NoProgress).unwrap();
        assert_eq!(*fake.downloads.borrow(), 1, "an intact cache is used");

        // Someone changes the cached file.
        fs::write(dirs(&root).cache_dir().join(cache_file_name(&job)), b"evil").unwrap();
        install_archive(&fake, &dirs(&root), &project, &job, &mut NoProgress).unwrap();
        assert_eq!(*fake.downloads.borrow(), 2, "it was downloaded again");
        assert!(project.join("addons/gut/plugin.cfg").is_file());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn an_addon_archive_with_a_link_installs_nothing() {
        let root = scratch("addon-with-link");
        let project = root.join("project");
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        writer.start_file("o-r-abc/plugin.cfg", options).unwrap();
        writer.write_all(b"x").unwrap();
        writer
            .add_symlink("o-r-abc/link", "plugin.cfg", options)
            .unwrap();
        let fake = Fake {
            archive: writer.finish().unwrap().into_inner(),
            downloads: RefCell::new(0),
            urls: RefCell::new(vec![]),
        };
        let src = source();
        let result = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, Some("gut"), false),
            &mut NoProgress,
        );
        assert!(
            matches!(
                result,
                Err(AddonError::Archive(EngineError::UnsafeArchive(_)))
            ),
            "{result:?}"
        );
        assert!(!project.join("addons/gut").exists());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn status_compares_with_the_lockfile() {
        let root = scratch("addon-status");
        let project = root.join("project");
        let fake = Fake::new(&[("o-r-abc/addons/gut/plugin.cfg", "x")]);
        let src = source();
        let fetched = install_github(
            &fake,
            &dirs(&root),
            &project,
            &plan(&src, None, None, false),
            &mut NoProgress,
        )
        .unwrap();
        let locked = LockedAddon {
            name: fetched.name.clone(),
            source: src.lock_name(),
            rev: SHA.into(),
            subdir: fetched.subdir.clone(),
            hash: fetched.hash.clone(),
        };
        assert_eq!(
            addon_status(&project, &locked).unwrap(),
            AddonStatus::Installed
        );
        fs::write(project.join("addons/gut/extra.gd"), "edited").unwrap();
        assert_eq!(
            addon_status(&project, &locked).unwrap(),
            AddonStatus::Modified
        );
        remove_addon_dir(&project, "gut").unwrap();
        assert_eq!(
            addon_status(&project, &locked).unwrap(),
            AddonStatus::Missing
        );
        remove_addon_dir(&project, "gut").unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
