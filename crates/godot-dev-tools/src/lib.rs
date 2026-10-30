//! Library behind the `gdt` command line tool.
//!
//! All logic lives here; the binaries are thin layers over it (see ADR 0001).
//! This crate never prints and never exits the process.

pub mod http;
pub mod progress;
pub mod project;
pub mod releases;
pub mod storage;
pub mod version;

pub use http::{HttpClient, HttpError, HttpResponse, UreqClient};
pub use progress::{NoProgress, Progress};
pub use project::{EngineFeatures, PROJECT_FILE, ProjectError, find_project_file};
pub use releases::{Asset, Release, ReleaseError, ResponseCache, Source, fetch_releases};
pub use storage::{Dirs, Platform, StorageError};
pub use version::{Build, Channel, Flavor, Target, Version, VersionError, VersionSpec};
