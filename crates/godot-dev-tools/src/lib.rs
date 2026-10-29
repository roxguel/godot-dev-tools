//! Library behind the `gdt` command line tool.
//!
//! All logic lives here; the binaries are thin layers over it (see ADR 0001).
//! This crate never prints and never exits the process.

pub mod project;

pub use project::{EngineFeatures, ProjectError};
