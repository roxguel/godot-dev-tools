//! Library behind the `gdt` command line tool.
//!
//! All logic lives here; the binaries are thin layers over it (see ADR 0001).
//! This crate never prints and never exits the process.

pub mod project;
pub mod storage;
pub mod version;

pub use project::{EngineFeatures, PROJECT_FILE, ProjectError, find_project_file};
pub use storage::{Dirs, Platform, StorageError};
pub use version::{Build, Channel, Flavor, Target, Version, VersionError, VersionSpec};
