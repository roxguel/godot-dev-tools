//! Showing progress to the user: rich bars, plain lines, or nothing.
//!
//! Everything here writes to stderr, so stdout stays clean for data.

use std::env;
use std::io::IsTerminal;
use std::time::Duration;

use clap::ValueEnum;
use godot_dev_tools::Progress;
use indicatif::{ProgressBar, ProgressDrawTarget, ProgressStyle};

/// The `--progress` choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ProgressArg {
    /// Rich in a terminal, plain otherwise
    Auto,
    /// Spinners and progress bars redrawn in place
    Rich,
    /// One line per step
    Plain,
    /// Nothing
    #[value(name = "none")]
    Off,
}

/// The style actually used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Rich,
    Plain,
    Off,
}

impl Style {
    /// Decides the style from the flags and the terminal.
    pub fn resolve(arg: ProgressArg, quiet: bool) -> Self {
        let in_ci = env::var_os("CI").is_some_and(|v| !v.is_empty());
        let dumb = env::var("TERM").is_ok_and(|t| t == "dumb");
        Self::decide(arg, quiet, std::io::stderr().is_terminal(), in_ci, dumb)
    }

    fn decide(arg: ProgressArg, quiet: bool, is_terminal: bool, in_ci: bool, dumb: bool) -> Self {
        if quiet {
            return Self::Off;
        }
        match arg {
            ProgressArg::Rich => Self::Rich,
            ProgressArg::Plain => Self::Plain,
            ProgressArg::Off => Self::Off,
            ProgressArg::Auto if is_terminal && !in_ci && !dumb => Self::Rich,
            ProgressArg::Auto => Self::Plain,
        }
    }
}

/// A [`Progress`] that draws on stderr in the chosen [`Style`].
pub struct CliProgress {
    style: Style,
    bar: Option<ProgressBar>,
}

impl CliProgress {
    pub fn new(style: Style) -> Self {
        Self { style, bar: None }
    }

    fn template(template: &str) -> ProgressStyle {
        ProgressStyle::with_template(template)
            .unwrap_or_else(|_| ProgressStyle::default_bar())
            .progress_chars("=> ")
    }
}

impl Progress for CliProgress {
    fn begin(&mut self, message: &str, total: Option<u64>) {
        match self.style {
            Style::Rich => {
                let bar = match total {
                    Some(total) => ProgressBar::new(total).with_style(Self::template(
                        "{msg}\n[{bar:40.cyan/blue}] {bytes}/{total_bytes}  {bytes_per_sec}  ETA {eta}",
                    )),
                    None => ProgressBar::new_spinner().with_style(Self::template("{spinner} {msg}")),
                };
                bar.set_draw_target(ProgressDrawTarget::stderr());
                bar.set_message(message.to_string());
                bar.enable_steady_tick(Duration::from_millis(100));
                self.bar = Some(bar);
            }
            Style::Plain => eprintln!("{message}..."),
            Style::Off => {}
        }
    }

    fn message(&mut self, text: &str) {
        match (&self.bar, self.style) {
            (Some(bar), _) => bar.set_message(text.to_string()),
            (None, Style::Plain) => eprintln!("{text}..."),
            _ => {}
        }
    }

    fn advance(&mut self, amount: u64) {
        if let Some(bar) = &self.bar {
            bar.inc(amount);
        }
    }

    fn finish(&mut self, message: &str) {
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
        if self.style != Style::Off {
            eprintln!("{message}");
        }
    }
}

impl Drop for CliProgress {
    fn drop(&mut self) {
        // An error mid-step must not leave a half-drawn bar behind.
        if let Some(bar) = self.bar.take() {
            bar.finish_and_clear();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quiet_wins_over_everything() {
        assert_eq!(
            Style::decide(ProgressArg::Rich, true, true, false, false),
            Style::Off
        );
    }

    #[test]
    fn explicit_choices_are_respected() {
        assert_eq!(
            Style::decide(ProgressArg::Rich, false, false, true, true),
            Style::Rich
        );
        assert_eq!(
            Style::decide(ProgressArg::Plain, false, true, false, false),
            Style::Plain
        );
        assert_eq!(
            Style::decide(ProgressArg::Off, false, true, false, false),
            Style::Off
        );
    }

    #[test]
    fn auto_is_rich_only_in_a_real_terminal() {
        assert_eq!(
            Style::decide(ProgressArg::Auto, false, true, false, false),
            Style::Rich
        );
        assert_eq!(
            Style::decide(ProgressArg::Auto, false, false, false, false),
            Style::Plain
        );
        assert_eq!(
            Style::decide(ProgressArg::Auto, false, true, true, false),
            Style::Plain
        );
        assert_eq!(
            Style::decide(ProgressArg::Auto, false, true, false, true),
            Style::Plain
        );
    }
}
