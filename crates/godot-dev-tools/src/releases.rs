//! Discovering Godot releases on GitHub (see `docs/specs/version-management.md`).
//!
//! Stable builds are in `godotengine/godot`; pre-releases (beta, rc, dev) are in
//! `godotengine/godot-builds`. Release lists are cached on disk for a short time
//! so repeated commands do not exhaust GitHub's anonymous rate limit.

use std::fs;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde::Deserialize;

use crate::http::{HttpClient, HttpError};
use crate::progress::Progress;
use crate::version::{Channel, Version};

/// Errors from fetching or reading release lists.
#[derive(Debug, thiserror::Error)]
pub enum ReleaseError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("GitHub sent a release list that could not be read: {0}")]
    Json(String),
}

/// Which GitHub repository to read releases from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Stable releases, from `godotengine/godot`.
    Stable,
    /// Pre-releases (beta, rc, dev), from `godotengine/godot-builds`.
    PreReleases,
}

impl Source {
    pub fn repo(self) -> &'static str {
        match self {
            Self::Stable => "godotengine/godot",
            Self::PreReleases => "godotengine/godot-builds",
        }
    }

    fn first_page_url(self) -> String {
        format!(
            "https://api.github.com/repos/{}/releases?per_page=100",
            self.repo()
        )
    }

    /// How many pages of 100 releases are read. Enough for every 4.x release;
    /// older Godot 3 releases further back are not needed.
    fn max_pages(self) -> usize {
        match self {
            Self::Stable => 3,
            Self::PreReleases => 2,
        }
    }

    fn wants(self, version: &Version) -> bool {
        match self {
            Self::Stable => version.channel == Channel::Stable,
            Self::PreReleases => version.channel != Channel::Stable,
        }
    }
}

/// One downloadable file of a release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

/// A Godot release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub tag: String,
    pub source: Source,
    pub assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    #[serde(default)]
    size: u64,
}

/// Reads one page of GitHub's release list. Drafts, tags that are not Godot 4.x
/// versions (including every Godot 3 tag) and releases of the wrong channel for
/// `source` are skipped.
pub fn parse_page(source: Source, body: &[u8]) -> Result<Vec<Release>, ReleaseError> {
    let raw: Vec<RawRelease> =
        serde_json::from_slice(body).map_err(|e| ReleaseError::Json(e.to_string()))?;
    Ok(raw
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| {
            let version = Version::from_tag(&r.tag_name).ok()?;
            source.wants(&version).then(|| Release {
                version,
                tag: r.tag_name,
                source,
                assets: r
                    .assets
                    .into_iter()
                    .map(|a| Asset {
                        name: a.name,
                        url: a.browser_download_url,
                        size: a.size,
                    })
                    .collect(),
            })
        })
        .collect())
}

/// The URL of the next page from a `Link` header, if any.
pub fn next_link(header: &str) -> Option<String> {
    header.split(',').find_map(|part| {
        let (url, params) = part.split_once(';')?;
        params
            .split(';')
            .any(|p| p.trim() == "rel=\"next\"")
            .then(|| {
                url.trim()
                    .trim_start_matches('<')
                    .trim_end_matches('>')
                    .to_string()
            })
    })
}

/// One cached page of a release list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedPage {
    pub next: Option<String>,
    pub body: Vec<u8>,
}

/// A small on-disk cache of release-list pages, keyed by URL.
#[derive(Debug, Clone)]
pub struct ResponseCache {
    dir: PathBuf,
    ttl: Duration,
}

impl ResponseCache {
    pub fn new(dir: PathBuf, ttl: Duration) -> Self {
        Self { dir, ttl }
    }

    fn path(&self, url: &str) -> PathBuf {
        let mut hasher = DefaultHasher::new();
        url.hash(&mut hasher);
        self.dir.join(format!("{:016x}.page", hasher.finish()))
    }

    /// The cached page for `url`, if there is one younger than the time to live.
    pub fn read(&self, url: &str) -> Option<CachedPage> {
        let path = self.path(url);
        let age = fs::metadata(&path).ok()?.modified().ok()?.elapsed().ok()?;
        if age > self.ttl {
            return None;
        }
        let bytes = fs::read(path).ok()?;
        let split = bytes.iter().position(|&b| b == b'\n')?;
        let next = String::from_utf8(bytes[..split].to_vec()).ok()?;
        Some(CachedPage {
            next: (!next.is_empty()).then_some(next),
            body: bytes[split + 1..].to_vec(),
        })
    }

    /// Stores a page. Failing to write the cache is harmless; callers may ignore it.
    pub fn write(&self, url: &str, page: &CachedPage) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut bytes = page.next.clone().unwrap_or_default().into_bytes();
        bytes.push(b'\n');
        bytes.extend_from_slice(&page.body);
        fs::write(self.path(url), bytes)
    }
}

/// The releases in a cached page, or `None` if the entry cannot be trusted: its
/// link to the next page must point at the GitHub API and its body must read.
/// A cache file can be cut short by a crash or edited by hand.
fn usable_page(source: Source, data: &CachedPage) -> Option<Vec<Release>> {
    let next_ok = data
        .next
        .as_deref()
        .is_none_or(|next| next.starts_with("https://api.github.com/"));
    if !next_ok {
        return None;
    }
    parse_page(source, &data.body).ok()
}

/// Fetches the releases of `source`, newest first as GitHub returns them.
/// With `refresh`, the cache is bypassed for reading (and still updated).
pub fn fetch_releases(
    http: &dyn HttpClient,
    cache: Option<&ResponseCache>,
    source: Source,
    refresh: bool,
    progress: &mut dyn Progress,
) -> Result<Vec<Release>, ReleaseError> {
    let mut releases = Vec::new();
    let mut url = Some(source.first_page_url());

    for page in 1..=source.max_pages() {
        let Some(current) = url.take() else { break };
        progress.message(&format!(
            "Fetching {} releases (page {page})",
            source.repo()
        ));

        let cached = cache
            .filter(|_| !refresh)
            .and_then(|c| c.read(&current))
            .and_then(|data| match usable_page(source, &data) {
                Some(page) => Some((data, page)),
                None => {
                    log::debug!("ignoring a damaged cache entry for {current}");
                    None
                }
            });
        let (data, page) = match cached {
            Some(found) => {
                log::debug!("release list served from the cache: {current}");
                found
            }
            None => {
                let response = http.get(&current)?;
                let data = CachedPage {
                    next: response.header("link").and_then(next_link),
                    body: response.body,
                };
                // Only a page that reads is worth keeping.
                let page = parse_page(source, &data.body)?;
                if let Some(cache) = cache {
                    let _ = cache.write(&current, &data);
                }
                (data, page)
            }
        };

        releases.extend(page);
        progress.advance(1);
        url = data.next;
    }
    Ok(releases)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use crate::progress::NoProgress;
    use std::cell::RefCell;

    const PAGE: &str = r#"[
        {"tag_name":"4.7.2-stable","draft":false,"assets":[
            {"name":"Godot_v4.7.2-stable_win64.exe.zip","browser_download_url":"https://example.com/win.zip","size":86013866},
            {"name":"SHA512-SUMS.txt","browser_download_url":"https://example.com/sums","size":5682}]},
        {"tag_name":"4.8-beta2","draft":false,"assets":[]},
        {"tag_name":"4.9-dev1","draft":true,"assets":[]},
        {"tag_name":"3.6.3-stable","draft":false,"assets":[]},
        {"tag_name":"not-a-version","draft":false,"assets":[]},
        {"tag_name":"4.7-stable","assets":[]}
    ]"#;

    fn v(tag: &str) -> Version {
        Version::from_tag(tag).unwrap()
    }

    #[test]
    fn stable_pages_keep_only_stable_godot_4() {
        let releases = parse_page(Source::Stable, PAGE.as_bytes()).unwrap();
        let versions: Vec<_> = releases.iter().map(|r| r.version).collect();
        assert_eq!(versions, [v("4.7.2-stable"), v("4.7-stable")]);
        assert_eq!(releases[0].assets.len(), 2);
        assert_eq!(releases[0].assets[0].size, 86013866);
        assert_eq!(releases[0].tag, "4.7.2-stable");
    }

    #[test]
    fn pre_release_pages_keep_only_pre_releases_and_skip_drafts() {
        let releases = parse_page(Source::PreReleases, PAGE.as_bytes()).unwrap();
        let versions: Vec<_> = releases.iter().map(|r| r.version).collect();
        assert_eq!(versions, [v("4.8-beta2")]);
    }

    #[test]
    fn garbage_is_a_json_error() {
        assert!(matches!(
            parse_page(Source::Stable, b"<html>"),
            Err(ReleaseError::Json(_))
        ));
    }

    #[test]
    fn next_link_is_read_from_the_header() {
        let header = "<https://api.github.com/r?page=2>; rel=\"next\", <https://api.github.com/r?page=9>; rel=\"last\"";
        assert_eq!(
            next_link(header).as_deref(),
            Some("https://api.github.com/r?page=2")
        );
        assert_eq!(next_link("<https://x/?page=1>; rel=\"prev\""), None);
        assert_eq!(next_link(""), None);
    }

    /// Answers from a script and counts how many requests were made.
    struct Fake {
        pages: RefCell<Vec<HttpResponse>>,
        calls: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(pages: Vec<HttpResponse>) -> Self {
            Self {
                pages: RefCell::new(pages),
                calls: RefCell::new(vec![]),
            }
        }
    }

    impl HttpClient for Fake {
        fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
            self.calls.borrow_mut().push(url.to_string());
            let mut pages = self.pages.borrow_mut();
            if pages.is_empty() {
                return Err(HttpError::Status {
                    url: url.into(),
                    status: 404,
                });
            }
            Ok(pages.remove(0))
        }

        fn download(
            &self,
            url: &str,
            _sink: &mut dyn std::io::Write,
            _on_chunk: &mut dyn FnMut(Option<u64>, u64),
        ) -> Result<(), HttpError> {
            unreachable!("release lists are never downloaded as files: {url}")
        }
    }

    fn page(body: &str, next: Option<&str>) -> HttpResponse {
        let headers = next
            .map(|n| vec![("Link".to_string(), format!("<{n}>; rel=\"next\""))])
            .unwrap_or_default();
        HttpResponse {
            status: 200,
            headers,
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn pages_are_followed_until_there_is_no_next_link() {
        let fake = Fake::new(vec![
            page(PAGE, Some("https://api.github.com/page2")),
            page(r#"[{"tag_name":"4.6.3-stable","assets":[]}]"#, None),
        ]);
        let releases = fetch_releases(&fake, None, Source::Stable, false, &mut NoProgress).unwrap();
        assert_eq!(releases.len(), 3);
        assert_eq!(fake.calls.borrow().len(), 2);
        assert_eq!(fake.calls.borrow()[1], "https://api.github.com/page2");
    }

    #[test]
    fn fetching_stops_at_the_page_limit() {
        let endless = |n: u32| page("[]", Some(&format!("https://api.github.com/p{n}")));
        let fake = Fake::new((1..=10).map(endless).collect());
        fetch_releases(&fake, None, Source::Stable, false, &mut NoProgress).unwrap();
        assert_eq!(fake.calls.borrow().len(), Source::Stable.max_pages());
    }

    #[test]
    fn http_errors_are_passed_on() {
        let fake = Fake::new(vec![]);
        let err = fetch_releases(&fake, None, Source::Stable, false, &mut NoProgress);
        assert!(matches!(
            err,
            Err(ReleaseError::Http(HttpError::Status { status: 404, .. }))
        ));
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gdt-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn the_cache_avoids_a_second_request_until_refreshed() {
        let dir = scratch("cache");
        let cache = ResponseCache::new(dir.clone(), Duration::from_secs(600));

        let first = Fake::new(vec![page(PAGE, None)]);
        let a =
            fetch_releases(&first, Some(&cache), Source::Stable, false, &mut NoProgress).unwrap();
        assert_eq!(first.calls.borrow().len(), 1);

        let second = Fake::new(vec![]);
        let b = fetch_releases(
            &second,
            Some(&cache),
            Source::Stable,
            false,
            &mut NoProgress,
        )
        .unwrap();
        assert_eq!(second.calls.borrow().len(), 0, "served from the cache");
        assert_eq!(a, b);

        let third = Fake::new(vec![page(PAGE, None)]);
        fetch_releases(&third, Some(&cache), Source::Stable, true, &mut NoProgress).unwrap();
        assert_eq!(third.calls.borrow().len(), 1, "refresh bypasses the cache");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_damaged_cache_entry_is_discarded_and_fetched_again() {
        let dir = scratch("cache-damaged");
        let cache = ResponseCache::new(dir.clone(), Duration::from_secs(600));
        let url = Source::Stable.first_page_url();
        for (next, body) in [
            ("", "not json at all"),
            ("", "[{\"tag_name\":"),
            ("https://evil.example/next", PAGE),
            ("", ""),
        ] {
            fs::create_dir_all(&dir).unwrap();
            fs::write(cache.path(&url), format!("{next}\n{body}")).unwrap();
            let fake = Fake::new(vec![page(PAGE, None)]);
            let releases =
                fetch_releases(&fake, Some(&cache), Source::Stable, false, &mut NoProgress)
                    .unwrap();
            assert!(!releases.is_empty(), "{next:?} {body:?}");
            assert_eq!(fake.calls.borrow().len(), 1, "{next:?} {body:?}");
            // The bad entry was replaced by the good one.
            let again = Fake::new(vec![]);
            fetch_releases(&again, Some(&cache), Source::Stable, false, &mut NoProgress).unwrap();
            assert_eq!(again.calls.borrow().len(), 0);
        }
        // A reply that does not read is not kept.
        let _ = fs::remove_file(cache.path(&url));
        let bad = Fake::new(vec![page("garbage", None)]);
        assert!(
            fetch_releases(&bad, Some(&cache), Source::Stable, false, &mut NoProgress).is_err()
        );
        assert!(cache.read(&url).is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_expired_cache_entry_is_ignored() {
        let dir = scratch("expired");
        let cache = ResponseCache::new(dir.clone(), Duration::ZERO);
        let data = CachedPage {
            next: Some("n".into()),
            body: b"[]".to_vec(),
        };
        cache.write("u", &data).unwrap();
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(cache.read("u"), None);

        let fresh = ResponseCache::new(dir.clone(), Duration::from_secs(60));
        assert_eq!(fresh.read("u"), Some(data));
        assert_eq!(fresh.read("other"), None);
        let _ = fs::remove_dir_all(dir);
    }
}
