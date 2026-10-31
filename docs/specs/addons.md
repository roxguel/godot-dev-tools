# Addons

Status: Draft. Implemented: the Asset Store, the Asset Library and GitHub sources, the lockfile and the `.gitignore` block (`gdt addon add|install|remove|list|update|search`, `gdt sync`). Not yet: URL and path sources, and signing in to the Store (paid assets).

## Manifest and lockfile

Addons are declared in the project's `gdt.toml` and pinned in `gdt.lock`; both are committed.

```toml
[addons]
git = "managed"                 # "managed" (default), "track" or "ignore": see "Git and the addons folder"

[addons.cogito]
store = "philip-drobar/cogito"  # Asset Store: publisher/asset
version = "1.2.0"               # optional: without it, the newest compatible stable release

[addons.gut]
github = "bitwes/Gut"
ref = "v9.4.0"      # a tag, a branch or a commit; the default branch if absent
subdir = "addons/gut"           # folder inside the repo to copy into addons/<name>/

[addons.some-lib]
library = "1234"                # legacy Asset Library asset id
version = "2.1"

[addons.other]
url = "https://example.com/other-1.0.zip"
sha256 = "..."                  # required for url sources

[addons.gdunit4]
store = "mikeschulze/gdunit4"
dev = true                      # needed to work on the project, not shipped: see "Development addons"
```

`gdt.lock` records the exact resolved source: commit SHA for GitHub, release id and download details for stores, plus the content hash of the extracted files for every source. `gdt addon install` reproduces the lock exactly and fails if a hash does not match.

## Sources

| Source | Command form | Notes |
|---|---|---|
| Godot Asset Store (default) | `gdt addon add publisher/asset`, `store:publisher/asset@version` | Public API, no sign-in for free assets. Assets are `publisher/asset` slugs, not numbers. Without a version, the newest **stable** release that works with the project's Godot (`min_godot_version` and `max_godot_version`, asked through the API's `compatibility` filter); with `@version` (a leading `v` is optional) exactly that release. The lockfile records the release version. A paid asset is refused with its store address (exit code 6): `gdt` never buys anything |
| Godot Asset Library (legacy) | `library:1234` | REST API; numeric ids. The lockfile records the version the Library lists; if the Library lists another one later, `install` stops and says to run `update`. When the author gave a SHA-256 for the archive, it is checked |
| GitHub | `github:owner/repo@tag`, or a full `https://github.com/...` URL | By `ref`: a tag, a branch or a commit. `subdir` selects the addon folder inside the repo. Private repositories need a token: see [authentication.md](authentication.md) |
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

## Development addons

Like Node's `devDependencies` or Cargo's `[dev-dependencies]`: an addon marked `dev = true` is needed to work on the project (a test framework such as GUT or gdUnit4, an editor-only tool) but not by the game that ships.

```toml
[addons.gut]
github = "bitwes/Gut"
ref = "v9.4.0"      # a tag, a branch or a commit; the default branch if absent
dev = true
```

What the flag does:

- `gdt sync --no-dev` and `gdt addon install --no-dev` skip them, so a release job installs only what the game needs. Everything else installs them by default.
- `gdt test` needs its runner: if the runner's addon is `dev` and was skipped, it says so instead of failing obscurely.
- `gdt addon export-filter` prints the `exclude_filter` entries for the development addons (for example `addons/gut/*`) to paste into an export preset.

Why this is more than Godot's own export filters: a preset's `exclude_filter` is the only thing that keeps an addon out of an export, and it is written by hand per preset. The default export mode includes every resource in the project, so a test framework ships unless someone excluded it. Declaring the addon once as `dev` is the single source for both "do not install it in release jobs" and "do not ship it".

`gdt` does not rewrite `export_presets.cfg` by default: the editor rewrites that file whenever it saves, so edits from outside are easily lost. A `--write` option may come later, to be used with the editor closed.

## Installation rules

- An addon is installed into `addons/<name>/`. Nothing outside `addons/` is written, except the files `gdt` owns (`gdt.toml`, `gdt.lock`, and its `.gitignore` block).
- Names and folders are checked wherever they come from (the command line, `gdt.toml`, `gdt.lock`), because `gdt` creates and deletes `addons/<name>/` and moves `subdir` out of the archive. A name is one plain folder name: no separators, no leading dot, no trailing space or dot, no characters Windows forbids, and not a reserved Windows name (`con`, `nul`, `com1`...). A `subdir` is a relative path of plain folder names (`.` means the whole archive): no absolute paths, no drive letters, no `..`. A file that breaks these rules is refused as a whole, with the offending name in the message.
- Enabling the plugin (`editor_plugins/enabled` in `project.godot`) is **not** done automatically; `gdt addon add` prints the line to add, or enables it with `--enable`.
- No transitive dependencies in the first version.
- Paid assets are never purchased by `gdt`; see [authentication.md](authentication.md).

## Open design points

- Addons that ship extra top-level files (GDExtension binaries, `.gdextension` per platform).
- Submodule support for GitHub sources.
- Private repositories: delegate to the user's git configuration.
