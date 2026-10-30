//! How long-running work reports progress.
//!
//! The library never prints (ADR 0001): it reports through this trait and the
//! caller decides how to show it (a spinner, a bar, plain lines, nothing).

/// Receives progress from a long-running operation.
pub trait Progress {
    /// Starts a step. `total` is the amount of work (for example bytes) when known.
    fn begin(&mut self, message: &str, total: Option<u64>);
    /// Changes the text of the current step.
    fn message(&mut self, text: &str);
    /// Records that `amount` more units of work are done.
    fn advance(&mut self, amount: u64);
    /// Ends the current step with a closing message.
    fn finish(&mut self, message: &str);
}

/// A [`Progress`] that ignores everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn begin(&mut self, _message: &str, _total: Option<u64>) {}
    fn message(&mut self, _text: &str) {}
    fn advance(&mut self, _amount: u64) {}
    fn finish(&mut self, _message: &str) {}
}
