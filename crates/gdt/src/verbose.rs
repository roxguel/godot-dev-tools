//! `-v` / `-vv`: diagnostic output on stderr.
//!
//! The library reports what it does through the `log` crate; this is where it
//! becomes visible. `-v` shows gdt's own messages (URLs, chosen assets, cache
//! hits, paths, checksums); `-vv` also shows what the libraries underneath say
//! (HTTP and TLS). Tokens are never logged by gdt.

use log::{Level, LevelFilter, Log, Metadata, Record};

struct StderrLogger {
    everything: bool,
}

fn is_ours(target: &str) -> bool {
    target.starts_with("godot_dev_tools") || target.starts_with("gdt")
}

impl Log for StderrLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        self.everything || is_ours(metadata.target())
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
        eprintln!("[{level}] {}", record.args());
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
    });
    if log::set_logger(Box::leak(logger)).is_ok() {
        log::set_max_level(if level >= 2 {
            LevelFilter::Trace
        } else {
            LevelFilter::Debug
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_one_shows_only_our_own_messages() {
        let logger = StderrLogger { everything: false };
        let ours = Metadata::builder()
            .target("godot_dev_tools::engine")
            .build();
        let cli = Metadata::builder().target("gdt").build();
        let other = Metadata::builder().target("rustls::client").build();
        assert!(logger.enabled(&ours));
        assert!(logger.enabled(&cli));
        assert!(!logger.enabled(&other));
        assert!(StderrLogger { everything: true }.enabled(&other));
    }
}
