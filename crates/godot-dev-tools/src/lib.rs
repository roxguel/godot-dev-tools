//! Library behind the `gdt` command line tool.
//!
//! All logic lives here; the binaries are thin layers over it (see ADR 0001).
//! This crate never prints and never exits the process. Diagnostics go through
//! the `log` crate; the caller decides whether and how to show them.

pub mod config;
pub mod engine;
pub mod http;
pub mod manifest;
pub mod progress;
pub mod project;
pub mod releases;
pub mod resolve;
pub mod storage;
pub mod version;

pub use config::{Config, ConfigError};
pub use engine::{
    Arch, EngineError, InstallRequest, Installed, find_console_executable, find_executable,
    install_build, uninstall_build,
};
pub use http::{HttpClient, HttpError, HttpResponse, UreqClient};
pub use manifest::{MANIFEST_FILE, Manifest, ManifestError, pin};
pub use progress::{NoProgress, Progress};
pub use project::{EngineFeatures, PROJECT_FILE, ProjectError, find_project_file};
pub use releases::{Asset, Release, ReleaseError, ResponseCache, Source, fetch_releases};
pub use resolve::{
    ProjectRoot, Request, ResolveError, Resolved, find_project_root, pick_installed,
    removal_candidates, resolve,
};
pub use storage::{Dirs, Platform, StorageError};
pub use version::{
    Build, Channel, ChannelKind, Flavor, Target, Version, VersionError, VersionSpec,
};
