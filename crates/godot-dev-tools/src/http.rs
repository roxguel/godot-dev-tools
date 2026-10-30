//! A small HTTP layer so the rest of the library can be tested without a network.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GITHUB_API: &str = "https://api.github.com/";
const MAX_BODY_BYTES: u64 = 64 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

/// Errors from an HTTP request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("network error for {url}: {message}")]
    Network { url: String, message: String },
    #[error("{url} answered with HTTP {status}")]
    Status { url: String, status: u16 },
    #[error("{}", rate_limit_message(*reset_at, now_secs()))]
    RateLimited {
        /// Unix time at which the limit resets, when GitHub said so.
        reset_at: Option<u64>,
    },
}

/// A successful (2xx) response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    /// The value of a header, matched without regard to case.
    pub fn header(&self, name: &str) -> Option<&str> {
        header_value(&self.headers, name)
    }
}

/// Something that can fetch a URL.
pub trait HttpClient {
    /// Fetches `url`. Only 2xx answers are returned as `Ok`.
    fn get(&self, url: &str) -> Result<HttpResponse, HttpError>;
}

fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

/// Turns a status code into `Ok` or the matching error; recognizes GitHub's
/// rate limit (HTTP 429, or 403 with no requests remaining).
pub fn check_status(url: &str, status: u16, headers: &[(String, String)]) -> Result<(), HttpError> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let exhausted = header_value(headers, "x-ratelimit-remaining") == Some("0");
    if status == 429 || (status == 403 && exhausted) {
        return Err(HttpError::RateLimited {
            reset_at: header_value(headers, "x-ratelimit-reset").and_then(|v| v.parse().ok()),
        });
    }
    Err(HttpError::Status {
        url: url.to_string(),
        status,
    })
}

/// The message shown when the rate limit is hit.
pub fn rate_limit_message(reset_at: Option<u64>, now: u64) -> String {
    let when = match reset_at {
        Some(at) if at > now => {
            let minutes = (at - now).div_ceil(60);
            format!(" It resets in about {minutes} minute(s).")
        }
        _ => String::new(),
    };
    format!(
        "GitHub API rate limit reached.{when} Set GITHUB_TOKEN (or GH_TOKEN) to raise the limit."
    )
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The real client, built on `ureq` (TLS through rustls and `ring`).
pub struct UreqClient {
    agent: ureq::Agent,
    token: Option<String>,
}

impl UreqClient {
    /// `token` is an optional GitHub token. It is sent only to `api.github.com`,
    /// never to any other host, and is never printed.
    pub fn new(user_agent: &str, token: Option<String>) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(user_agent)
            .http_status_as_error(false)
            .timeout_global(Some(TIMEOUT))
            .build();
        Self {
            agent: config.into(),
            token,
        }
    }
}

impl HttpClient for UreqClient {
    fn get(&self, url: &str) -> Result<HttpResponse, HttpError> {
        let network = |e: ureq::Error| HttpError::Network {
            url: url.to_string(),
            message: e.to_string(),
        };

        let mut request = self.agent.get(url);
        if url.starts_with(GITHUB_API) {
            request = request.header("Accept", "application/vnd.github+json");
            if let Some(token) = &self.token {
                request = request.header("Authorization", &format!("Bearer {token}"));
            }
        }

        let mut response = request.call().map_err(network)?;
        let status = response.status().as_u16();
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .filter_map(|(k, v)| Some((k.as_str().to_string(), v.to_str().ok()?.to_string())))
            .collect();
        check_status(url, status, &headers)?;

        let body = response
            .body_mut()
            .with_config()
            .limit(MAX_BODY_BYTES)
            .read_to_vec()
            .map_err(network)?;
        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn success_statuses_pass() {
        assert_eq!(check_status("u", 200, &[]), Ok(()));
        assert_eq!(check_status("u", 204, &[]), Ok(()));
    }

    #[test]
    fn other_errors_report_the_status() {
        assert_eq!(
            check_status("https://x/y", 404, &[]),
            Err(HttpError::Status {
                url: "https://x/y".into(),
                status: 404
            })
        );
    }

    #[test]
    fn exhausted_403_is_a_rate_limit() {
        let headers = h(&[
            ("X-RateLimit-Remaining", "0"),
            ("X-RateLimit-Reset", "1700000000"),
        ]);
        assert_eq!(
            check_status("u", 403, &headers),
            Err(HttpError::RateLimited {
                reset_at: Some(1_700_000_000)
            })
        );
    }

    #[test]
    fn plain_403_is_not_a_rate_limit() {
        assert!(matches!(
            check_status("u", 403, &[]),
            Err(HttpError::Status { status: 403, .. })
        ));
        let headers = h(&[("x-ratelimit-remaining", "12")]);
        assert!(matches!(
            check_status("u", 403, &headers),
            Err(HttpError::Status { .. })
        ));
    }

    #[test]
    fn too_many_requests_is_a_rate_limit() {
        assert_eq!(
            check_status("u", 429, &[]),
            Err(HttpError::RateLimited { reset_at: None })
        );
    }

    #[test]
    fn the_rate_limit_message_says_when_it_resets() {
        let msg = rate_limit_message(Some(1_000 + 125), 1_000);
        assert!(msg.contains("3 minute"), "{msg}");
        assert!(msg.contains("GITHUB_TOKEN"));
        assert!(!rate_limit_message(None, 1_000).contains("resets"));
        assert!(!rate_limit_message(Some(10), 1_000).contains("resets"));
    }

    #[test]
    fn headers_are_found_without_regard_to_case() {
        let r = HttpResponse {
            status: 200,
            headers: h(&[("Link", "x")]),
            body: vec![],
        };
        assert_eq!(r.header("link"), Some("x"));
        assert_eq!(r.header("etag"), None);
    }
}
