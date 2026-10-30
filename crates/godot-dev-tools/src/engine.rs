//! Installing Godot builds (see `docs/specs/version-management.md`).
//!
//! The steps are: choose the asset for this machine from an explicit table,
//! read its SHA-512 from the release's `SHA512-SUMS.txt`, download it into the
//! cache while hashing, verify, extract into a temporary folder and move that
//! folder into place, so a failed install never leaves a half-installed build.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha512};

use crate::http::{HttpClient, HttpError};
use crate::progress::Progress;
use crate::releases::{Asset, Release};
use crate::storage::{Dirs, Platform};
use crate::version::{Build, Flavor};

const SUMS_FILE: &str = "SHA512-SUMS.txt";

/// Errors from installing a build.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Http(#[from] HttpError),
    /// Nothing published matches what was asked for.
    #[error("no Godot release matches `{0}`; see `gdt list --remote --all`")]
    VersionNotFound(String),
    /// The release has no build for this machine.
    #[error("this release has no build for {0}")]
    NoBuild(String),
    #[error("the release does not include {SUMS_FILE}, so the download cannot be verified")]
    NoChecksumFile,
    #[error("{SUMS_FILE} does not list {0}, so the download cannot be verified")]
    NoChecksum(String),
    #[error("checksum mismatch for {name}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("unsafe path in the archive: {0}")]
    UnsafeArchive(String),
    #[error("could not read the archive: {0}")]
    Archive(String),
    #[error("{context}: {source}")]
    Io {
        context: String,
        #[source]
        source: io::Error,
    },
}

fn io_err(context: impl Into<String>) -> impl FnOnce(io::Error) -> EngineError {
    let context = context.into();
    move |source| EngineError::Io { context, source }
}

/// The processor architecture of the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

impl Arch {
    /// The architecture this binary was built for.
    pub fn current() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Aarch64
        } else {
            Self::X86_64
        }
    }
}

/// The release asset for a platform, architecture and flavor, or `None` when
/// Godot does not publish one. Names are listed explicitly rather than built
/// from a pattern because the separators differ between builds.
pub fn asset_name(tag: &str, flavor: Flavor, platform: Platform, arch: Arch) -> Option<String> {
    let mono = flavor == Flavor::Dotnet;
    let suffix = match (platform, arch, mono) {
        (Platform::Windows, Arch::X86_64, false) => "win64.exe.zip",
        (Platform::Windows, Arch::X86_64, true) => "mono_win64.zip",
        (Platform::Windows, Arch::Aarch64, false) => "windows_arm64.exe.zip",
        (Platform::Windows, Arch::Aarch64, true) => "mono_windows_arm64.zip",
        (Platform::Linux, Arch::X86_64, false) => "linux.x86_64.zip",
        (Platform::Linux, Arch::X86_64, true) => "mono_linux_x86_64.zip",
        (Platform::Linux, Arch::Aarch64, false) => "linux.arm64.zip",
        (Platform::Linux, Arch::Aarch64, true) => "mono_linux_arm64.zip",
        (Platform::MacOs, _, false) => "macos.universal.zip",
        (Platform::MacOs, _, true) => "mono_macos.universal.zip",
    };
    Some(format!("Godot_v{tag}_{suffix}"))
}

/// Reads a `SHA512-SUMS.txt`: lines of `<hex>  <file name>`.
pub fn parse_sums(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let (hash, name) = line.trim().split_once(char::is_whitespace)?;
            let name = name.trim().trim_start_matches('*');
            let is_sha512 = hash.len() == 128 && hash.bytes().all(|b| b.is_ascii_hexdigit());
            (is_sha512 && !name.is_empty()).then(|| (name.to_string(), hash.to_ascii_lowercase()))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes to `inner` while hashing everything written.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha512,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let written = self.inner.write(buf)?;
        self.hasher.update(&buf[..written]);
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buffer = vec![0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buffer[..read]);
    }
}

/// Makes sure the verified archive for `asset` is in the cache and returns its
/// path. A cached archive is reused only if it still matches `expected`.
fn fetch_archive(
    http: &dyn HttpClient,
    asset: &Asset,
    expected: &str,
    cache_dir: &Path,
    progress: &mut dyn Progress,
) -> Result<PathBuf, EngineError> {
    let dest = cache_dir.join(&asset.name);
    if dest.is_file() {
        match hash_file(&dest) {
            Ok(actual) if actual == expected => {
                log::debug!("using the cached archive {}", dest.display());
                return Ok(dest);
            }
            _ => {
                log::debug!("the cached archive does not match; downloading again");
                let _ = fs::remove_file(&dest);
            }
        }
    }

    fs::create_dir_all(cache_dir).map_err(io_err("could not create the cache folder"))?;
    let part = cache_dir.join(format!("{}.part", asset.name));
    let file = File::create(&part).map_err(io_err("could not create the download file"))?;
    let mut writer = HashingWriter {
        inner: BufWriter::new(file),
        hasher: Sha512::new(),
    };

    progress.begin(
        &format!("Downloading {}", asset.name),
        (asset.size > 0).then_some(asset.size),
    );
    log::debug!(
        "downloading {} ({} bytes) from {}",
        asset.name,
        asset.size,
        asset.url
    );
    let result = http.download(&asset.url, &mut writer, &mut |_total, read| {
        progress.advance(read)
    });
    let flushed = writer.flush();
    let actual = hex(&writer.hasher.finalize());
    drop(writer.inner);

    if let Err(error) = result {
        let _ = fs::remove_file(&part);
        return Err(error.into());
    }
    if let Err(error) = flushed {
        let _ = fs::remove_file(&part);
        return Err(io_err("could not write the download")(error));
    }
    progress.finish(&format!("Downloaded {}", asset.name));

    log::debug!("sha512 expected {expected}");
    log::debug!("sha512 actual   {actual}");
    if actual != expected {
        let _ = fs::remove_file(&part);
        return Err(EngineError::ChecksumMismatch {
            name: asset.name.clone(),
            expected: expected.to_string(),
            actual,
        });
    }
    fs::rename(&part, &dest).map_err(io_err("could not store the download"))?;
    Ok(dest)
}

/// Extracts a zip into `dest`. Entries that would land outside `dest` (absolute
/// paths, `..`) and symbolic links that point outside it are rejected.
pub fn extract_zip(archive: &Path, dest: &Path) -> Result<(), EngineError> {
    let file = File::open(archive).map_err(io_err("could not open the archive"))?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| EngineError::Archive(e.to_string()))?;
    fs::create_dir_all(dest).map_err(io_err("could not create the install folder"))?;

    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| EngineError::Archive(e.to_string()))?;
        let relative = entry.enclosed_name().ok_or_else(|| {
            EngineError::UnsafeArchive(String::from_utf8_lossy(entry.name_raw()).into_owned())
        })?;
        let out = dest.join(&relative);
        let context = format!("could not extract {}", relative.display());

        if entry.is_dir() {
            fs::create_dir_all(&out).map_err(io_err(context))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(io_err(context.clone()))?;
        }

        if entry.is_symlink() {
            let mut target = String::new();
            entry
                .read_to_string(&mut target)
                .map_err(io_err(context.clone()))?;
            make_symlink(&target, &out, &relative)?;
            continue;
        }

        let mut file = File::create(&out).map_err(io_err(context.clone()))?;
        io::copy(&mut entry, &mut file).map_err(io_err(context.clone()))?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o777))
                .map_err(io_err(context))?;
        }
    }
    Ok(())
}

/// Whether a link target stays inside the folder it was extracted into.
#[cfg(any(unix, test))]
fn symlink_target_is_safe(target: &str, link_relative: &Path) -> bool {
    use std::path::Component;
    let target = Path::new(target);
    if target.is_absolute() || target.has_root() {
        return false;
    }
    // Depth of the folder that contains the link, below the extraction root.
    let mut depth = link_relative.components().count().saturating_sub(1) as i64;
    for component in target.components() {
        match component {
            Component::ParentDir => depth -= 1,
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            _ => return false,
        }
        if depth < 0 {
            return false;
        }
    }
    true
}

#[cfg(unix)]
fn make_symlink(target: &str, out: &Path, relative: &Path) -> Result<(), EngineError> {
    if !symlink_target_is_safe(target, relative) {
        return Err(EngineError::UnsafeArchive(format!(
            "{} -> {target}",
            relative.display()
        )));
    }
    std::os::unix::fs::symlink(target, out)
        .map_err(io_err(format!("could not link {}", relative.display())))
}

#[cfg(not(unix))]
fn make_symlink(target: &str, _out: &Path, relative: &Path) -> Result<(), EngineError> {
    Err(EngineError::UnsafeArchive(format!(
        "{} -> {target} (symbolic links are not supported on this platform)",
        relative.display()
    )))
}

/// Finds the editor executable inside an installed build.
pub fn find_executable(dir: &Path, platform: Platform) -> Option<PathBuf> {
    let mut folders = vec![dir.to_path_buf()];
    folders.extend(
        fs::read_dir(dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir()),
    );

    for folder in folders {
        if platform == Platform::MacOs {
            if let Some(app) = fs::read_dir(&folder)
                .ok()?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.extension().is_some_and(|x| x == "app"))
            {
                let macos = app.join("Contents").join("MacOS");
                if let Some(exe) = fs::read_dir(macos)
                    .ok()
                    .and_then(|mut d| d.find_map(|e| e.ok().map(|e| e.path())))
                {
                    return Some(exe);
                }
            }
            continue;
        }

        let mut candidates: Vec<PathBuf> = fs::read_dir(&folder)
            .ok()?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_file() && is_editor_executable(p, platform))
            .collect();
        candidates.sort();
        if let Some(exe) = candidates.into_iter().next() {
            return Some(exe);
        }
    }
    None
}

fn is_editor_executable(path: &Path, platform: Platform) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    match platform {
        // The editor, not the `_console.exe` twin that keeps a terminal attached.
        Platform::Windows => name.ends_with(".exe") && !name.contains("console"),
        _ => name.starts_with("Godot") && is_executable_file(path),
    }
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable_file(_path: &Path) -> bool {
    true
}

/// What to install.
pub struct InstallRequest<'a> {
    pub release: &'a Release,
    pub flavor: Flavor,
    pub platform: Platform,
    pub arch: Arch,
}

/// The result of an install.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    pub build: Build,
    pub dir: PathBuf,
    pub executable: Option<PathBuf>,
    /// The build was already installed and nothing was downloaded.
    pub already_installed: bool,
}

/// Installs one build, verified and atomic. Does nothing if it is installed.
pub fn install_build(
    http: &dyn HttpClient,
    dirs: &Dirs,
    request: &InstallRequest<'_>,
    progress: &mut dyn Progress,
) -> Result<Installed, EngineError> {
    let InstallRequest {
        release,
        flavor,
        platform,
        arch,
    } = *request;
    let build = Build {
        version: release.version,
        flavor,
    };
    let target = dirs.build_dir(&build);
    let installed = |already_installed| Installed {
        build,
        executable: find_executable(&target, platform),
        dir: target.clone(),
        already_installed,
    };
    if target.is_dir() {
        return Ok(installed(true));
    }

    let wanted = asset_name(&release.tag, flavor, platform, arch)
        .and_then(|name| release.assets.iter().find(|a| a.name == name))
        .ok_or_else(|| {
            EngineError::NoBuild(format!(
                "{platform:?} {arch:?} ({flavor:?}) in {}",
                release.tag
            ))
        })?;
    log::debug!("selected asset {}", wanted.name);

    let sums = release
        .assets
        .iter()
        .find(|a| a.name == SUMS_FILE)
        .ok_or(EngineError::NoChecksumFile)?;
    progress.message("Reading the checksums");
    let sums = http.get(&sums.url)?;
    let sums = parse_sums(&String::from_utf8_lossy(&sums.body));
    let expected = sums
        .get(&wanted.name)
        .ok_or_else(|| EngineError::NoChecksum(wanted.name.clone()))?;

    let archive = fetch_archive(http, wanted, expected, &dirs.cache_dir(), progress)?;

    progress.begin(&format!("Installing Godot {build}"), None);
    let versions = dirs.versions_dir();
    fs::create_dir_all(&versions).map_err(io_err("could not create the versions folder"))?;
    let staging = versions.join(format!(".tmp-{build}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&staging);
    if let Err(error) = extract_zip(&archive, &staging) {
        let _ = fs::remove_dir_all(&staging);
        return Err(error);
    }
    log::debug!("extracted to {}", staging.display());

    if let Err(error) = fs::rename(&staging, &target) {
        let _ = fs::remove_dir_all(&staging);
        if target.is_dir() {
            // Another gdt installed the same build at the same moment.
            return Ok(installed(true));
        }
        return Err(io_err("could not move the build into place")(error));
    }
    progress.finish(&format!("Installed Godot {build}"));
    Ok(installed(false))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use crate::progress::NoProgress;
    use crate::releases::Source;
    use crate::version::Version;
    use std::cell::RefCell;
    use std::io::Cursor;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        let options = zip::write::SimpleFileOptions::default();
        for (name, data) in entries {
            writer.start_file(*name, options).unwrap();
            writer.write_all(data).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha512::digest(bytes))
    }

    #[test]
    fn asset_names_follow_the_published_table() {
        let t = "4.7.2-stable";
        let name = |f, p, a| asset_name(t, f, p, a).unwrap();
        assert_eq!(
            name(Flavor::Standard, Platform::Windows, Arch::X86_64),
            "Godot_v4.7.2-stable_win64.exe.zip"
        );
        assert_eq!(
            name(Flavor::Dotnet, Platform::Windows, Arch::X86_64),
            "Godot_v4.7.2-stable_mono_win64.zip"
        );
        assert_eq!(
            name(Flavor::Standard, Platform::Linux, Arch::X86_64),
            "Godot_v4.7.2-stable_linux.x86_64.zip"
        );
        assert_eq!(
            name(Flavor::Dotnet, Platform::Linux, Arch::Aarch64),
            "Godot_v4.7.2-stable_mono_linux_arm64.zip"
        );
        assert_eq!(
            name(Flavor::Standard, Platform::MacOs, Arch::Aarch64),
            "Godot_v4.7.2-stable_macos.universal.zip"
        );
        assert_eq!(
            asset_name(
                "4.8-beta2",
                Flavor::Standard,
                Platform::Windows,
                Arch::Aarch64
            )
            .unwrap(),
            "Godot_v4.8-beta2_windows_arm64.exe.zip"
        );
    }

    #[test]
    fn sums_are_read_by_file_name() {
        let upper = "AB".repeat(64);
        let lower = "cd".repeat(64);
        let text = format!("{upper}  one.zip\n{lower} *two.zip\n\nnot a line\nabc123  short.zip\n");
        let sums = parse_sums(&text);
        assert_eq!(sums.get("one.zip"), Some(&upper.to_ascii_lowercase()));
        assert_eq!(sums.get("two.zip"), Some(&lower));
        assert_eq!(sums.len(), 2, "lines that are not SHA-512 sums are ignored");
    }

    #[test]
    fn extraction_creates_files_and_folders() {
        let dir = scratch("extract-ok");
        let archive = dir.join("a.zip");
        fs::write(
            &archive,
            zip_bytes(&[("Godot.exe", b"exe"), ("GodotSharp/lib.dll", b"dll")]),
        )
        .unwrap();
        let out = dir.join("out");
        extract_zip(&archive, &out).unwrap();
        assert_eq!(fs::read(out.join("Godot.exe")).unwrap(), b"exe");
        assert_eq!(
            fs::read(out.join("GodotSharp").join("lib.dll")).unwrap(),
            b"dll"
        );
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn extraction_rejects_paths_that_escape() {
        let dir = scratch("extract-slip");
        let archive = dir.join("evil.zip");
        fs::write(&archive, zip_bytes(&[("../evil.txt", b"x")])).unwrap();
        let out = dir.join("out");
        let result = extract_zip(&archive, &out);
        assert!(
            matches!(result, Err(EngineError::UnsafeArchive(_))),
            "{result:?}"
        );
        assert!(!dir.join("evil.txt").exists());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn symlink_targets_must_stay_inside() {
        let at = |target: &str, link: &str| symlink_target_is_safe(target, Path::new(link));
        assert!(at("Versions/A", "Frameworks/X.framework/Current"));
        assert!(at("../Resources", "App/Contents/MacOS/link"));
        assert!(!at("../../../../etc", "App/link"));
        assert!(!at("/etc/passwd", "link"));
        assert!(!at("..", "link"));
    }

    #[test]
    fn the_windows_editor_is_found_not_its_console_twin() {
        let dir = scratch("find-win");
        fs::write(dir.join("Godot_v4.7.2-stable_win64_console.exe"), b"").unwrap();
        fs::write(dir.join("Godot_v4.7.2-stable_win64.exe"), b"").unwrap();
        let found = find_executable(&dir, Platform::Windows).unwrap();
        assert!(
            found.ends_with("Godot_v4.7.2-stable_win64.exe"),
            "{found:?}"
        );

        let nested = scratch("find-win-nested");
        fs::create_dir_all(nested.join("Godot_mono")).unwrap();
        fs::write(nested.join("Godot_mono").join("Godot_mono_win64.exe"), b"").unwrap();
        assert!(find_executable(&nested, Platform::Windows).is_some());
        assert_eq!(
            find_executable(&scratch("find-win-empty"), Platform::Windows),
            None
        );
        let _ = fs::remove_dir_all(dir);
        let _ = fs::remove_dir_all(nested);
    }

    /// Serves a fixed archive and checksum list, counting downloads.
    struct Fake {
        archive: Vec<u8>,
        sums: String,
        downloads: RefCell<u32>,
    }

    impl HttpClient for Fake {
        fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
            assert_eq!(url, "mem://sums");
            Ok(HttpResponse {
                status: 200,
                headers: vec![],
                body: self.sums.clone().into_bytes(),
            })
        }

        fn download(
            &self,
            url: &str,
            sink: &mut dyn Write,
            on_chunk: &mut dyn FnMut(Option<u64>, u64),
        ) -> Result<(), HttpError> {
            assert_eq!(url, "mem://archive");
            *self.downloads.borrow_mut() += 1;
            sink.write_all(&self.archive).unwrap();
            on_chunk(Some(self.archive.len() as u64), self.archive.len() as u64);
            Ok(())
        }
    }

    fn release() -> Release {
        let asset = |name: &str, url: &str| Asset {
            name: name.into(),
            url: url.into(),
            size: 10,
        };
        Release {
            version: Version::from_tag("4.7.2-stable").unwrap(),
            tag: "4.7.2-stable".into(),
            source: Source::Stable,
            assets: vec![
                asset("Godot_v4.7.2-stable_win64.exe.zip", "mem://archive"),
                asset("SHA512-SUMS.txt", "mem://sums"),
            ],
        }
    }

    fn dirs(name: &str) -> (Dirs, PathBuf) {
        let root = scratch(name);
        let dirs = Dirs::resolve(Platform::Windows, |k| {
            (k == "GDT_HOME").then(|| root.clone().into_os_string())
        })
        .unwrap();
        (dirs, root)
    }

    fn request(release: &Release) -> InstallRequest<'_> {
        InstallRequest {
            release,
            flavor: Flavor::Standard,
            platform: Platform::Windows,
            arch: Arch::X86_64,
        }
    }

    #[test]
    fn a_verified_archive_is_installed_once() {
        let (dirs, root) = dirs("install-ok");
        let archive = zip_bytes(&[("Godot_v4.7.2-stable_win64.exe", b"exe")]);
        let fake = Fake {
            sums: format!("{}  Godot_v4.7.2-stable_win64.exe.zip\n", sha(&archive)),
            archive,
            downloads: RefCell::new(0),
        };
        let release = release();

        let first = install_build(&fake, &dirs, &request(&release), &mut NoProgress).unwrap();
        assert!(!first.already_installed);
        assert!(first.dir.join("Godot_v4.7.2-stable_win64.exe").is_file());
        assert!(first.executable.is_some());
        assert_eq!(first.build.to_string(), "4.7.2");

        let second = install_build(&fake, &dirs, &request(&release), &mut NoProgress).unwrap();
        assert!(second.already_installed);
        assert_eq!(*fake.downloads.borrow(), 1, "nothing is downloaded twice");
        assert_eq!(dirs.installed_builds().unwrap().len(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_cached_archive_is_reused_after_an_uninstall() {
        let (dirs, root) = dirs("install-cache");
        let archive = zip_bytes(&[("Godot_v4.7.2-stable_win64.exe", b"exe")]);
        let fake = Fake {
            sums: format!("{}  Godot_v4.7.2-stable_win64.exe.zip\n", sha(&archive)),
            archive,
            downloads: RefCell::new(0),
        };
        let release = release();
        let first = install_build(&fake, &dirs, &request(&release), &mut NoProgress).unwrap();
        fs::remove_dir_all(first.dir).unwrap();
        install_build(&fake, &dirs, &request(&release), &mut NoProgress).unwrap();
        assert_eq!(
            *fake.downloads.borrow(),
            1,
            "the cached archive was verified and reused"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_wrong_checksum_installs_nothing() {
        let (dirs, root) = dirs("install-bad");
        let fake = Fake {
            archive: zip_bytes(&[("Godot.exe", b"exe")]),
            sums: format!("{}  Godot_v4.7.2-stable_win64.exe.zip\n", "0".repeat(128)),
            downloads: RefCell::new(0),
        };
        let release = release();
        let result = install_build(&fake, &dirs, &request(&release), &mut NoProgress);
        assert!(
            matches!(result, Err(EngineError::ChecksumMismatch { .. })),
            "{result:?}"
        );
        assert_eq!(dirs.installed_builds().unwrap(), vec![]);
        let leftovers: Vec<_> = fs::read_dir(dirs.cache_dir())
            .map(|d| d.filter_map(Result::ok).collect())
            .unwrap_or_default();
        assert!(
            leftovers.is_empty(),
            "no archive or partial file is kept: {leftovers:?}"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_missing_checksum_line_is_an_error() {
        let (dirs, root) = dirs("install-nosum");
        let fake = Fake {
            archive: vec![],
            sums: "abc  some_other_file.zip\n".into(),
            downloads: RefCell::new(0),
        };
        let release = release();
        let result = install_build(&fake, &dirs, &request(&release), &mut NoProgress);
        assert!(matches!(result, Err(EngineError::NoChecksum(_))));
        assert_eq!(
            *fake.downloads.borrow(),
            0,
            "nothing is downloaded without a checksum"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn a_release_without_this_platform_says_so() {
        let (dirs, root) = dirs("install-nobuild");
        let fake = Fake {
            archive: vec![],
            sums: String::new(),
            downloads: RefCell::new(0),
        };
        let release = release();
        let mut req = request(&release);
        req.platform = Platform::Linux;
        let result = install_build(&fake, &dirs, &req, &mut NoProgress);
        assert!(matches!(result, Err(EngineError::NoBuild(_))));
        let _ = fs::remove_dir_all(root);
    }
}
