//! Library behind the `gdt` command line tool.
//!
//! All logic lives here; the binaries are thin layers over it (see ADR 0001).
//! This crate never prints and never exits the process. Diagnostics go through
//! the `log` crate; the caller decides whether and how to show them.

pub mod addons;
pub mod config;
pub mod engine;
pub mod guard;
pub mod http;
pub mod lock;
pub mod manifest;
pub mod names;
pub mod progress;
pub mod project;
pub mod releases;
pub mod resolve;
pub mod storage;
pub mod stores;
pub mod testing;
pub mod version;

pub use addons::{
    AddonError, AddonStatus, ArchiveJob, Fetched, GithubSource, InstallPlan, addon_status,
    install_archive, install_github, remove_addon_dir, resolve_commit,
};
pub use config::{Config, ConfigError};
pub use engine::{
    Arch, EngineError, InstallRequest, Installed, find_console_executable, find_executable,
    install_build, uninstall_build,
};
pub use http::{HttpClient, HttpError, HttpResponse, UreqClient};
pub use lock::{
    LOCK_FILE, LockError, LockFile, LockedAddon, hash_dir, ignore_lines, set_ignore_block,
};
pub use manifest::{
    AddonEntry, GitMode, MANIFEST_FILE, Manifest, ManifestError, TestConfig, pin, remove_addon,
    set_addon,
};
pub use progress::{NoProgress, Progress};
pub use project::{EngineFeatures, PROJECT_FILE, ProjectError, find_project_file};
pub use releases::{Asset, Release, ReleaseError, ResponseCache, Source, fetch_releases};
pub use resolve::{
    ProjectRoot, Request, ResolveError, Resolved, find_project_root, godot_path_arg,
    pick_installed, removal_candidates, resolve,
};
pub use storage::{Dirs, Platform, StorageError};
pub use stores::{
    LibraryAsset, SearchHit, StoreAsset, StoreInfo, StoreRelease, library_asset, library_search,
    parse_library_id, pick_release, store_info, store_releases, store_search,
};
pub use testing::{
    Outcome, Runner, Summary, TestError, TestPlan, build_plan, detect_runner, judge, parse_junit,
    res_path,
};
pub use version::{
    Build, Channel, ChannelFilter, ChannelKind, Flavor, Target, Version, VersionError, VersionSpec,
};
