# Addons

Status: Draft. The Asset Store details come from its OpenAPI document (see [authentication.md](authentication.md)).

## Manifest and lockfile

Addons are declared in the project's `gdt.toml` and pinned in `gdt.lock`; both are committed.

```toml
[addons]
git = "managed"                 # "managed" (default), "track" or "ignore": see "Git and the addons folder"

[addons.cogito]
store = "philip-drobar/cogito"  # Asset Store: publisher/asset
version = "1.2.0"

[addons.gut]
github = "bitwes/Gut"
tag = "v9.4.0"
subdir = "addons/gut"           # folder inside the repo to copy into addons/<name>/

[addons.some-lib]
library = "1234"                # legacy Asset Library asset id
version = "2.1"

[addons.other]
url = "https://example.com/other-1.0.zip"
sha256 = "..."                  # required for url sources
```

`gdt.lock` records the exact resolved source: commit SHA for GitHub, release id and download details for stores, plus the content hash of the extracted files for every source. `gdt addon install` reproduces the lock exactly and fails if a hash does not match.

## Sources

| Source | Command form | Notes |
|---|---|---|
| Godot Asset Store (default) | `gdt addon add publisher/asset`, `store:publisher/asset` | Public API. Assets are identified by `publisher/asset` slugs, not numbers. Each release has `min_godot_version` and `max_godot_version`, so `gdt` picks the release compatible with the project's engine |
| Godot Asset Library (legacy) | `library:1234` | REST API; numeric ids |
| GitHub | `github:owner/repo@tag`, or a full `https://github.com/...` URL | By `tag`, `branch` or `rev`. `subdir` selects the addon folder inside the repo. Private repositories need a token: see [authentication.md](authentication.md) |
| URL | `https://example.com/x.zip` | Archive with a mandatory checksum |
| Path | `path:../my-addon` | Local folder, copied or linked; for development |

A bare `publisher/asset` always means the Asset Store. GitHub must be spelled `github:` (or a full URL), so the two forms never collide.

## Integrity

The Asset Store publishes **no checksum** for a release. `gdt` records the hash of the downloaded archive in `gdt.lock` the first time (trust on first use) and verifies every later install against it. GitHub sources are pinned by commit SHA.

## Git and the addons folder

Unlike `node_modules`, `addons/` is part of how Godot opens a project, so there are three modes, chosen with `[addons] git`:

| Mode | What Git sees | When to use it |
|---|---|---|
| `managed` (default) | Only the folders `gdt` installed are ignored, through a block `gdt` maintains in `.gitignore`. Addons added by hand, or modified by you, stay in Git | Most projects: reproducible installs without losing hand-made addons |
| `track` | Nothing is ignored; every addon folder is committed | The project must open without `gdt` or without a network. The lockfile still records and verifies hashes |
| `ignore` | The whole `addons/` folder is ignored; everything must come from `gdt` | Projects whose addons are all declared in `gdt.toml`. `gdt` warns when `addons/` holds a folder it does not manage, because that folder would not be in Git |

In `managed` and `ignore`, `gdt sync` or `gdt addon install` restores the addons: a fresh clone needs the network and that command before the editor opens cleanly. `gdt` refuses to overwrite a managed addon that has local changes (its hash no longer matches) unless `--force` is given.

## Installation rules

- An addon is installed into `addons/<name>/`. Nothing outside `addons/` is written, except the files `gdt` owns (`gdt.toml`, `gdt.lock`, and its `.gitignore` block).
- Enabling the plugin (`editor_plugins/enabled` in `project.godot`) is **not** done automatically; `gdt addon add` prints the line to add, or enables it with `--enable`.
- No transitive dependencies in the first version.
- Paid assets are never purchased by `gdt`; see [authentication.md](authentication.md).

## Open design points

- Addons that ship extra top-level files (GDExtension binaries, `.gdextension` per platform).
- Submodule support for GitHub sources.
- Private repositories: delegate to the user's git configuration.
