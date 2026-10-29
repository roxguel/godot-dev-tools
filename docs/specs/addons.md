# Addons

Status: Draft

## Manifest and lockfile

Addons are declared in the project's `gdt.toml` and pinned in `gdt.lock`, both committed.

```toml
[addons.gut]
git = "https://github.com/bitwes/Gut"
tag = "v9.4.0"
subdir = "addons/gut"          # folder inside the repo to copy into addons/<name>/

[addons.some-lib]
asset-library = "1234"          # legacy Asset Library asset id
version = "2.1"

[addons.other]
url = "https://example.com/other-1.0.zip"
sha256 = "..."                  # required for url sources
```

`gdt.lock` records the exact resolved source: commit SHA for git, asset id plus version and download URL for stores, content hash of the extracted files for all. `gdt install` reproduces the lock exactly and fails if a hash does not match.

## Sources

| Source | Notes |
|---|---|
| `git` | Any public (or accessible via the user's git) repository, by `tag`, `branch` or `rev`. `subdir` selects which folder of the repo is the addon; if absent and the repo has a single folder under `addons/`, that one is used |
| `asset-library` | The legacy Godot Asset Library (REST API) |
| `asset-store` | The new Godot Asset Store; depends on research ([open-questions.md](../open-questions.md)) |
| `url` | A zip or tarball with a mandatory checksum |
| `path` | A local folder, copied or symlinked; for development |

Shorthand on the command line: `gdt add github:owner/repo@tag`, `gdt add gitlab:owner/repo`, `gdt add asset-library:1234`.

## Installation rules

- An addon is installed into `addons/<name>/` of the project. Nothing outside `addons/` is written, except the files `gdt` itself owns (`gdt.toml`, `gdt.lock`).
- Enabling the plugin (`editor_plugins/enabled` in `project.godot`) is **not** done automatically; `gdt add` prints the line the user should add, or enables it with `--enable`.
- Installed addon folders are treated as vendored: `gdt` records a hash, and refuses to overwrite local modifications without `--force`.
- Whether `addons/` is committed or git-ignored is the project's choice. With a lockfile, either works.
- No transitive dependencies in the first version. An addon's own needs are documented by the addon.

## Open design points

- Handling addons that ship extra top-level files (GDExtension binaries, `.gdextension` files per platform).
- Submodule support for git sources.
- Authentication for private git repositories: delegate to the user's git configuration.
