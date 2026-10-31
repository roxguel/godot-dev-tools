//! `-v` / `-vv`: diagnostic output on stderr.
//!
//! The library reports what it does through the `log` crate; this is where it
//! becomes visible. `-v` shows gdt's own messages (URLs, chosen assets, cache
//! hits, paths, checksums); `-vv` also shows what the libraries underneath say
//! (HTTP and TLS), up to debug level: their trace output dumps raw headers.
//! Tokens are never logged by gdt, and anything that looks like one (an
//! `Authorization` header, the value of `GH_TOKEN`/`GITHUB_TOKEN`, GitHub token
//! formats) is replaced by `***` before a line is printed.

use log::{Level, LevelFilter, Log, Metadata, Record};

/// What replaces a secret in the output.
const MASK: &str = "***";

/// GitHub token prefixes (`ghp_`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `github_pat_`).
const TOKEN_PREFIXES: [&str; 6] = ["ghp_", "gho_", "ghu_", "ghs_", "ghr_", "github_pat_"];

/// The shortest run after a token prefix that is treated as a token.
const MIN_TOKEN_TAIL: usize = 8;

struct StderrLogger {
    everything: bool,
    /// Values that must never be printed, such as the tokens in the environment.
    secrets: Vec<String>,
}

fn is_ours(target: &str) -> bool {
    target.starts_with("godot_dev_tools") || target.starts_with("gdt")
}

fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~' | '+' | '/' | '=')
}

/// Replaces credentials in `text` with `***`: each of `known`, the rest of the
/// line after an `Authorization` header, and GitHub token formats.
fn redact(text: &str, known: &[String]) -> String {
    let mut out = text.to_string();
    for secret in known {
        if secret.len() >= MIN_TOKEN_TAIL {
            out = out.replace(secret.as_str(), MASK);
        }
    }
    mask_token_formats(&mask_authorization(&out))
}

/// Masks what follows `authorization` and a `:` or `=` up to the end of the line,
/// however the header was formatted (`Authorization: Bearer x`,
/// `"authorization": "Bearer x"`, `authorization = Basic x`).
fn mask_authorization(text: &str) -> String {
    const NAME: &str = "authorization";
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(NAME) {
        let name_end = at + found + NAME.len();
        let after_name = text[name_end..].trim_start_matches(['"', '\'', ' ']);
        let value_start = text.len() - after_name.len();
        if after_name.starts_with([':', '=']) {
            let value = &text[value_start + 1..];
            let value_end = value
                .find(['\n', '\r'])
                .map_or(text.len(), |end| value_start + 1 + end);
            out.push_str(&text[at..value_start + 1]);
            out.push(' ');
            out.push_str(MASK);
            at = value_end;
        } else {
            out.push_str(&text[at..name_end]);
            at = name_end;
        }
    }
    out.push_str(&text[at..]);
    out
}

fn mask_token_formats(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    'scan: while !rest.is_empty() {
        for prefix in TOKEN_PREFIXES {
            if let Some(tail) = rest.strip_prefix(prefix) {
                let len = tail.find(|c: char| !is_token_char(c)).unwrap_or(tail.len());
                if len >= MIN_TOKEN_TAIL {
                    out.push_str(MASK);
                    rest = &tail[len..];
                    continue 'scan;
                }
            }
        }
        let mut chars = rest.chars();
        if let Some(c) = chars.next() {
            out.push(c);
        }
        rest = chars.as_str();
    }
    out
}

fn secrets_from_env() -> Vec<String> {
    ["GH_TOKEN", "GITHUB_TOKEN"]
        .iter()
        .filter_map(|name| std::env::var(name).ok())
        .filter(|value| !value.is_empty())
        .collect()
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        if is_ours(metadata.target()) {
            return true;
        }
        // Libraries log raw headers at trace level, so only up to debug is shown.
        self.everything && metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let level = match record.level() {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
            Level::Trace => "trace",
        };
        eprintln!(
            "[{level}] {}",
            redact(&record.args().to_string(), &self.secrets)
        );
    }

    fn flush(&self) {}
}

/// Turns on diagnostics for `-v` (level 1) or `-vv` (level 2 and more).
pub fn init(level: u8) {
    if level == 0 {
        return;
    }
    let logger = Box::new(StderrLogger {
        everything: level >= 2,
        secrets: secrets_from_env(),
    });
    if log::set_logger(Box::leak(logger)).is_ok() {
        log::set_max_level(LevelFilter::Debug);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn logger(everything: bool) -> StderrLogger {
        StderrLogger {
            everything,
            secrets: vec![],
        }
    }

    #[test]
    fn level_one_shows_only_our_own_messages() {
        let ours = Metadata::builder()
            .target("godot_dev_tools::engine")
            .build();
        let cli = Metadata::builder().target("gdt").build();
        let other = Metadata::builder().target("rustls::client").build();
        assert!(logger(false).enabled(&ours));
        assert!(logger(false).enabled(&cli));
        assert!(!logger(false).enabled(&other));
        assert!(logger(true).enabled(&other));
    }

    #[test]
    fn level_two_keeps_libraries_below_trace() {
        let all = logger(true);
        let library = |level| Metadata::builder().target("ureq::run").level(level).build();
        assert!(all.enabled(&library(Level::Debug)));
        assert!(!all.enabled(&library(Level::Trace)));
    }

    #[test]
    fn credentials_are_masked() {
        let known = vec!["supersecretvalue".to_string()];
        for (input, leaked) in [
            ("Authorization: Bearer abc123def456", "abc123def456"),
            (
                "authorization: token abc123def456\r\nAccept: x",
                "abc123def456",
            ),
            ("\"authorization\": \"Bearer abc123def456\"", "abc123def456"),
            (
                "headers { Authorization = Basic dXNlcjpwYXNz }",
                "dXNlcjpwYXNz",
            ),
            (
                "?access=ghp_abcdefghijklmnopqrstuvwxyz0123456789",
                "ghp_abcdef",
            ),
            (
                "pat github_pat_11ABCDEFG0abcdefghijkl_zzzz",
                "github_pat_11",
            ),
            ("value is supersecretvalue here", "supersecretvalue"),
        ] {
            let shown = redact(input, &known);
            assert!(!shown.contains(leaked), "{input:?} -> {shown:?}");
            assert!(shown.contains(MASK), "{input:?} -> {shown:?}");
        }
    }

    #[test]
    fn text_without_secrets_is_left_alone() {
        let plain = "GET https://api.github.com/repos/o/r 200 OK ghost_ghp ghp_short";
        assert_eq!(redact(plain, &["supersecretvalue".to_string()]), plain);
        let shown = redact("authorization: Bearer abc123def456\r\nAccept: json", &[]);
        assert!(shown.contains("Accept: json"), "{shown:?}");
    }
}
