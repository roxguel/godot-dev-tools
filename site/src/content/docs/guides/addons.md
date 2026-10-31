---
title: Addons, gdt.toml and gdt.lock
description: Declare addons in gdt.toml, pin them in gdt.lock and restore them on any machine.
sidebar:
  order: 3
---

## Adding

```sh
gdt addon add github:bitwes/Gut@v9.7.1 --dev   # GitHub: a tag, a branch or a commit
gdt addon add publisher/asset                  # Godot Asset Store
gdt addon add library:1234                     # legacy Asset Library
gdt addon search dialogue                      # find one (--library for the old library)
```

`add` installs the addon into `addons/`, writes it to `gdt.toml` and pins it in `gdt.lock`.

```toml
[godot]
version = "4.7.2"

[addons.gut]
github = "bitwes/Gut"
ref = "v9.7.1"                     # a tag, a branch or a commit
dev = true                         # needed to work on the project, not shipped

[test]
runner = "gut"
paths = ["test"]
```

Keys `gdt` does not know are errors, so a typo is never ignored silently.

## The lockfile

`gdt.lock` records, for every addon, the exact commit or release and a hash of the installed files. Commit both files.

| Command | Does |
|---|---|
| `gdt addon install` | Installs exactly what the lock says. Refuses to overwrite a folder you changed unless `--force` is given |
| `gdt addon install --locked` | Fails instead of changing the lock |
| `gdt addon install --no-dev` | Skips `dev = true` addons (for a build that ships) |
| `gdt addon update [name]` | Looks for newer versions and rewrites the lock |
| `gdt addon remove <name>` | Removes it from the manifest, the lock and `addons/` |
| `gdt addon list` | What is installed and where it came from |
| `gdt sync` | On a fresh clone: the engine, then the addons |

## What goes in Git

By default `addons/` stays in Git except for the folders `gdt` installed, which it lists in a managed block of
`.gitignore`. `[addons] git = "track"` commits them too; `"ignore"` ignores the whole folder.

## Paid assets

`gdt` never buys anything. For a paid Asset Store asset it says where to get it and exits with 6.

Details: [addons spec](/specs/addons/).
