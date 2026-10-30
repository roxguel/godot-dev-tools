//! The Godot Asset Store and the legacy Godot Asset Library (see
//! `docs/specs/addons.md` and `docs/specs/authentication.md`).
//!
//! Both are read through their public APIs, without signing in, which is enough
//! for free assets. Neither publishes a checksum of what it serves (the Library
//! sometimes does), so the lockfile's hash of the installed files is what pins
//! them.

use serde_json::Value;

use crate::addons::AddonError;
use crate::http::{HttpClient, HttpError};

const STORE_API: &str = "https://store.godotengine.org/api/v1";
const LIBRARY_API: &str = "https://godotengine.org/asset-library/api";

fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

fn json(http: &dyn HttpClient, url: &str, what: &str) -> Result<Value, AddonError> {
    let response = match http.get(url) {
        Ok(response) => response,
        Err(HttpError::Status { status: 404, .. }) => {
            return Err(AddonError::NotFound(what.to_string()));
        }
        Err(e) => return Err(e.into()),
    };
    serde_json::from_slice(&response.body).map_err(|e| AddonError::BadReply(e.to_string()))
}

fn text(value: &Value, key: &str) -> Option<String> {
    match value.get(key)? {
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

// ---- The Asset Store -------------------------------------------------------

/// An asset of the Asset Store: `publisher/asset`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreAsset {
    pub publisher: String,
    pub asset: String,
}

impl StoreAsset {
    /// Reads `publisher/asset`, optionally with `@version` and a `store:` prefix.
    pub fn parse(source: &str) -> Result<(Self, Option<String>), AddonError> {
        let bad = || {
            AddonError::BadSource(format!(
                "`{source}` is not an Asset Store asset; write `publisher/asset` or `store:publisher/asset@version`"
            ))
        };
        let rest = source.trim();
        let rest = rest.strip_prefix("store:").unwrap_or(rest);
        let (slug, version) = match rest.rsplit_once('@') {
            Some((slug, v)) if !v.is_empty() => (slug, Some(v.to_string())),
            _ => (rest, None),
        };
        let (publisher, asset) = slug.split_once('/').ok_or_else(bad)?;
        let valid = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        };
        if !valid(publisher) || !valid(asset) {
            return Err(bad());
        }
        Ok((
            Self {
                publisher: publisher.to_string(),
                asset: asset.to_string(),
            },
            version,
        ))
    }

    pub fn slug(&self) -> String {
        format!("{}/{}", self.publisher, self.asset)
    }

    /// How the lockfile names this source.
    pub fn lock_name(&self) -> String {
        format!("store:{}", self.slug())
    }
}

/// What the Store says about an asset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreInfo {
    pub name: String,
    /// `0` for a free asset.
    pub price_cent: u64,
    pub license: Option<String>,
    pub store_url: String,
}

/// One downloadable release of an asset.
#[derive(Debug, Clone, PartialEq)]
pub struct StoreRelease {
    pub version: String,
    pub stable: bool,
    pub min_godot: Option<String>,
    pub max_godot: Option<String>,
    pub download_url: String,
}

/// Asks the Store about an asset.
pub fn store_info(http: &dyn HttpClient, asset: &StoreAsset) -> Result<StoreInfo, AddonError> {
    let url = format!("{STORE_API}/assets/{}/{}/", asset.publisher, asset.asset);
    let value = json(http, &url, &asset.slug())?;
    Ok(StoreInfo {
        name: text(&value, "name").unwrap_or_else(|| asset.asset.clone()),
        price_cent: value.get("price_cent").and_then(Value::as_u64).unwrap_or(0),
        license: text(&value, "license_type"),
        store_url: text(&value, "store_url").unwrap_or_else(|| {
            format!(
                "https://store.godotengine.org/asset/{}/{}/",
                asset.publisher, asset.asset
            )
        }),
    })
}

/// The releases of an asset, newest first as the Store lists them. With
/// `compatibility` (a Godot version such as `4.7`), only those that work with it.
pub fn store_releases(
    http: &dyn HttpClient,
    asset: &StoreAsset,
    compatibility: Option<&str>,
) -> Result<Vec<StoreRelease>, AddonError> {
    let mut url = format!("{STORE_API}/releases/{}/{}/", asset.publisher, asset.asset);
    if let Some(version) = compatibility {
        url.push_str(&format!("?compatibility={}", percent_encode(version)));
    }
    let value = json(http, &url, &asset.slug())?;
    let items = value
        .as_array()
        .ok_or_else(|| AddonError::BadReply("the release list is not a list".into()))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(StoreRelease {
                version: text(item, "version")?,
                stable: item.get("stable").and_then(Value::as_bool).unwrap_or(true),
                min_godot: text(item, "min_godot_version"),
                max_godot: text(item, "max_godot_version"),
                download_url: text(item, "download_url")?,
            })
        })
        .collect())
}

fn same_version(a: &str, b: &str) -> bool {
    let strip = |v: &str| v.trim().trim_start_matches(['v', 'V']).to_ascii_lowercase();
    strip(a) == strip(b)
}

/// The release a user means: the one named `wanted` (with or without a leading
/// `v`), or else the newest stable one (the newest of all if none is stable).
pub fn pick_release<'a>(
    releases: &'a [StoreRelease],
    wanted: Option<&str>,
) -> Option<&'a StoreRelease> {
    match wanted {
        Some(version) => releases.iter().find(|r| same_version(&r.version, version)),
        None => releases
            .iter()
            .find(|r| r.stable)
            .or_else(|| releases.first()),
    }
}

/// One result of a search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchHit {
    /// `publisher/asset` for the Store, the id for the Library.
    pub id: String,
    pub name: String,
    pub license: Option<String>,
    /// Free or the price in cents (the Store), `None` for the Library.
    pub price_cent: Option<u64>,
    /// The version or the Godot version, as the source gives it.
    pub detail: Option<String>,
}

/// Searches the Store.
pub fn store_search(
    http: &dyn HttpClient,
    query: &str,
    compatibility: Option<&str>,
) -> Result<Vec<SearchHit>, AddonError> {
    let mut url = format!(
        "{STORE_API}/search/query/?query={}&page_size=20",
        percent_encode(query)
    );
    if let Some(version) = compatibility {
        url.push_str(&format!("&compatibility={}", percent_encode(version)));
    }
    let value = json(http, &url, query)?;
    let hits = value
        .get("hits")
        .and_then(Value::as_array)
        .ok_or_else(|| AddonError::BadReply("no search results in the answer".into()))?;
    Ok(hits
        .iter()
        .filter_map(|hit| {
            let asset = hit.get("asset")?;
            let publisher = asset.get("publisher").and_then(|p| text(p, "slug"))?;
            Some(SearchHit {
                id: format!("{publisher}/{}", text(asset, "slug")?),
                name: text(asset, "name")?,
                license: text(asset, "license_type"),
                price_cent: asset.get("price_cent").and_then(Value::as_u64),
                detail: None,
            })
        })
        .collect())
}

// ---- The Asset Library (legacy) ----------------------------------------------------

/// An asset of the legacy Asset Library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryAsset {
    pub id: String,
    pub title: String,
    pub version: String,
    pub godot_version: Option<String>,
    pub license: Option<String>,
    pub download_url: String,
    /// The SHA-256 of the archive, when the author gave one.
    pub download_hash: Option<String>,
    pub browse_url: Option<String>,
}

impl LibraryAsset {
    /// How the lockfile names this source.
    pub fn lock_name(&self) -> String {
        format!("library:{}", self.id)
    }
}

/// Reads a Library id such as `5385`, with an optional `library:` prefix.
pub fn parse_library_id(source: &str) -> Result<String, AddonError> {
    let rest = source.trim();
    let rest = rest.strip_prefix("library:").unwrap_or(rest);
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AddonError::BadSource(format!(
            "`{source}` is not an Asset Library id; write `library:` followed by the number"
        )));
    }
    Ok(rest.to_string())
}

/// Asks the Library about an asset.
pub fn library_asset(http: &dyn HttpClient, id: &str) -> Result<LibraryAsset, AddonError> {
    let value = json(
        http,
        &format!("{LIBRARY_API}/asset/{id}"),
        &format!("library:{id}"),
    )?;
    if value.get("error").is_some() {
        return Err(AddonError::NotFound(format!("library:{id}")));
    }
    Ok(LibraryAsset {
        id: text(&value, "asset_id").unwrap_or_else(|| id.to_string()),
        title: text(&value, "title").unwrap_or_else(|| format!("library:{id}")),
        version: text(&value, "version_string")
            .or_else(|| text(&value, "version"))
            .ok_or_else(|| AddonError::BadReply("the asset has no version".into()))?,
        godot_version: text(&value, "godot_version"),
        license: text(&value, "cost"),
        download_url: text(&value, "download_url")
            .ok_or_else(|| AddonError::BadReply("the asset has no download".into()))?,
        download_hash: text(&value, "download_hash"),
        browse_url: text(&value, "browse_url"),
    })
}

/// Searches the Library for addons (optionally for one Godot version).
pub fn library_search(
    http: &dyn HttpClient,
    query: &str,
    godot_version: Option<&str>,
) -> Result<Vec<SearchHit>, AddonError> {
    let mut url = format!(
        "{LIBRARY_API}/asset?type=addon&max_results=20&filter={}",
        percent_encode(query)
    );
    // Without a Godot version the Library answers with nothing at all; `any`
    // is its word for "every version".
    url.push_str(&format!(
        "&godot_version={}",
        percent_encode(godot_version.unwrap_or("any"))
    ));
    let value = json(http, &url, query)?;
    let results = value
        .get("result")
        .and_then(Value::as_array)
        .ok_or_else(|| AddonError::BadReply("no search results in the answer".into()))?;
    Ok(results
        .iter()
        .filter_map(|item| {
            Some(SearchHit {
                id: format!("library:{}", text(item, "asset_id")?),
                name: text(item, "title")?,
                license: text(item, "cost"),
                price_cent: None,
                detail: match (text(item, "version_string"), text(item, "godot_version")) {
                    (Some(v), Some(g)) => Some(format!("{v}, Godot {g}")),
                    (Some(v), None) => Some(v),
                    (None, g) => g,
                },
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::HttpResponse;
    use std::cell::RefCell;
    use std::io::Write;

    /// Answers with the body registered for the first matching piece of the URL.
    struct Fake {
        routes: Vec<(&'static str, &'static str)>,
        seen: RefCell<Vec<String>>,
    }

    impl Fake {
        fn new(routes: Vec<(&'static str, &'static str)>) -> Self {
            Self {
                routes,
                seen: RefCell::new(vec![]),
            }
        }
    }

    impl HttpClient for Fake {
        fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
            self.seen.borrow_mut().push(url.to_string());
            match self.routes.iter().find(|(piece, _)| url.contains(piece)) {
                Some((_, body)) => Ok(HttpResponse {
                    status: 200,
                    headers: vec![],
                    body: body.as_bytes().to_vec(),
                }),
                None => Err(HttpError::Status {
                    url: url.into(),
                    status: 404,
                }),
            }
        }

        fn download(
            &self,
            _url: &str,
            _sink: &mut dyn Write,
            _on_chunk: &mut dyn FnMut(Option<u64>, u64),
        ) -> Result<(), HttpError> {
            unreachable!("these tests only read JSON")
        }
    }

    #[test]
    fn store_assets_are_parsed_in_every_spelling() {
        let (asset, version) = StoreAsset::parse("nathanhoad/dialogue-manager").unwrap();
        assert_eq!(asset.slug(), "nathanhoad/dialogue-manager");
        assert_eq!(version, None);
        assert_eq!(asset.lock_name(), "store:nathanhoad/dialogue-manager");

        let (_, version) = StoreAsset::parse("store:nathanhoad/dialogue-manager@v4.1.0").unwrap();
        assert_eq!(version.as_deref(), Some("v4.1.0"));
        for bad in ["", "solo", "a/b/c", "a/", "/b", "a b/c", "store:"] {
            assert!(
                matches!(StoreAsset::parse(bad), Err(AddonError::BadSource(_))),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn library_ids_are_numbers() {
        assert_eq!(parse_library_id("library:5385").unwrap(), "5385");
        assert_eq!(parse_library_id("5385").unwrap(), "5385");
        for bad in ["", "library:", "library:abc", "12x"] {
            assert!(parse_library_id(bad).is_err(), "{bad:?}");
        }
    }

    const RELEASES: &str = r#"[
        {"version":"v4.2.0-beta","stable":false,"min_godot_version":"4.6","max_godot_version":null,"download_url":"https://files/beta.zip"},
        {"version":"v4.1.0","stable":true,"min_godot_version":"4.6","max_godot_version":null,"download_url":"https://files/4.1.0.zip"},
        {"version":"v4.0.3","stable":true,"min_godot_version":"4.3","max_godot_version":"4.5","download_url":"https://files/4.0.3.zip"},
        {"stable":true,"download_url":"https://files/no-version.zip"}
    ]"#;

    #[test]
    fn releases_are_read_and_the_right_one_is_picked() {
        let fake = Fake::new(vec![("/releases/nathanhoad/dialogue-manager/", RELEASES)]);
        let (asset, _) = StoreAsset::parse("nathanhoad/dialogue-manager").unwrap();
        let releases = store_releases(&fake, &asset, Some("4.7")).unwrap();
        assert_eq!(releases.len(), 3, "an entry without a version is skipped");
        assert!(fake.seen.borrow()[0].ends_with("?compatibility=4.7"));

        let newest_stable = pick_release(&releases, None).unwrap();
        assert_eq!(newest_stable.version, "v4.1.0");
        assert_eq!(
            pick_release(&releases, Some("4.0.3")).unwrap().version,
            "v4.0.3"
        );
        assert!(!pick_release(&releases, Some("V4.2.0-BETA")).unwrap().stable);
        assert_eq!(pick_release(&releases, Some("9.9")), None);
        assert_eq!(releases[2].max_godot.as_deref(), Some("4.5"));
    }

    #[test]
    fn with_no_stable_release_the_newest_is_picked() {
        let only_beta = vec![StoreRelease {
            version: "v1".into(),
            stable: false,
            min_godot: None,
            max_godot: None,
            download_url: "u".into(),
        }];
        assert_eq!(pick_release(&only_beta, None).unwrap().version, "v1");
        assert_eq!(pick_release(&[], None), None);
    }

    #[test]
    fn asset_info_tells_free_from_paid() {
        let fake = Fake::new(vec![
            (
                "/assets/p/paid/",
                r#"{"name":"Paid Thing","price_cent":999,"license_type":"Proprietary","store_url":"https://store/asset/p/paid/"}"#,
            ),
            ("/assets/p/free/", r#"{"name":"Free Thing","price_cent":0}"#),
        ]);
        let paid = store_info(&fake, &StoreAsset::parse("p/paid").unwrap().0).unwrap();
        assert_eq!(
            (paid.price_cent, paid.store_url.as_str()),
            (999, "https://store/asset/p/paid/")
        );
        let free = store_info(&fake, &StoreAsset::parse("p/free").unwrap().0).unwrap();
        assert_eq!(free.price_cent, 0);
        assert!(free.store_url.contains("/asset/p/free/"));
        let missing = store_info(&fake, &StoreAsset::parse("p/nope").unwrap().0);
        assert!(matches!(missing, Err(AddonError::NotFound(_))));
    }

    #[test]
    fn store_search_reads_the_hits() {
        let fake = Fake::new(vec![(
            "/search/query/",
            r#"{"count":2,"hits":[
                {"asset":{"slug":"dialogue-manager","name":"Dialogue Manager","publisher":{"slug":"nathanhoad"},"price_cent":0,"license_type":"MIT"},"highlights":{}},
                {"asset":{"slug":"x","name":"X","publisher":{"slug":"p"},"price_cent":500}},
                {"nonsense":true}
            ]}"#,
        )]);
        let hits = store_search(&fake, "dialogue & more", Some("4.7")).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].id, "nathanhoad/dialogue-manager");
        assert_eq!(hits[0].license.as_deref(), Some("MIT"));
        assert_eq!(hits[1].price_cent, Some(500));
        let url = fake.seen.borrow()[0].clone();
        assert!(url.contains("query=dialogue%20%26%20more"), "{url}");
        assert!(url.contains("compatibility=4.7"), "{url}");
    }

    #[test]
    fn a_library_asset_is_read() {
        let fake = Fake::new(vec![
            (
                "/asset/5385",
                r#"{"asset_id":"5385","title":"DialogueDSL","version":"4","version_string":"1.3.2","godot_version":"4.6","cost":"Apache-2.0","download_url":"https://github.com/zHoeshin/DDSL/archive/33356a6.zip","download_hash":"","browse_url":"https://github.com/zHoeshin/DDSL"}"#,
            ),
            ("/asset/777", r#"{"error":"Asset not found"}"#),
        ]);
        let asset = library_asset(&fake, "5385").unwrap();
        assert_eq!(asset.version, "1.3.2");
        assert_eq!(
            asset.download_hash, None,
            "an empty hash means none was given"
        );
        assert_eq!(asset.lock_name(), "library:5385");
        assert!(matches!(
            library_asset(&fake, "777"),
            Err(AddonError::NotFound(_))
        ));
        assert!(matches!(
            library_asset(&fake, "1"),
            Err(AddonError::NotFound(_))
        ));
    }

    #[test]
    fn library_search_reads_the_results() {
        let fake = Fake::new(vec![(
            "/asset?",
            r#"{"result":[{"asset_id":"5385","title":"DialogueDSL","cost":"Apache-2.0","version_string":"1.3.2","godot_version":"4.6"}]}"#,
        )]);
        let hits = library_search(&fake, "dialogue", Some("4.7")).unwrap();
        assert_eq!(hits[0].id, "library:5385");
        assert_eq!(hits[0].detail.as_deref(), Some("1.3.2, Godot 4.6"));
        assert!(fake.seen.borrow()[0].contains("godot_version=4.7"));

        // With no Godot version asked for, it must still send one (`any`).
        library_search(&fake, "dialogue", None).unwrap();
        assert!(fake.seen.borrow()[1].contains("godot_version=any"));
    }

    #[test]
    fn query_text_is_percent_encoded() {
        assert_eq!(percent_encode("a b&c/ü"), "a%20b%26c%2F%C3%BC");
        assert_eq!(percent_encode("safe-_.~09"), "safe-_.~09");
    }
}
