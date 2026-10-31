---
title: Versions and resolution
description: Installing Godot builds, version specs, and which build gdt picks for a command.
sidebar:
  order: 1
---

## Installing

`gdt install <spec>` downloads an official build from the Godot releases on GitHub and checks it against the published
SHA-512 sums before installing it. Builds live in `gdt`'s own data folder (`gdt paths` shows where; `GDT_HOME` moves
it), one folder per build.

| Spec | Means |
|---|---|
| `4`, `4.7` | The newest stable build of that line. A pre-release is never picked by a loose spec |
| `4.7.2` | That exact release |
| `4.8-rc1` | That exact pre-release |
| `latest` | The newest stable build |
| `beta`, `rc`, `dev`, `pre` | The newest build of that channel (`pre`: of any pre-release channel) |
| `…-mono`, or `--mono` | The .NET build, for C# projects |

`gdt ls` shows installed builds, with what is in use here and the default; `gdt ls -r` shows what GitHub offers.
`gdt uninstall 4.6.3` removes one, and refuses moving names such as `latest`.

## Which build a command uses

The first match decides, and there is no silent fallback:

1. `--godot <version>` (`-g`)
2. the `GDT_VERSION` environment variable
3. `[godot] version` in the project's `gdt.toml`
4. the engine in `project.godot` (`config/features`; a C# project picks the .NET build)
5. the global default (`gdt use`)

The project is the nearest folder above the current one with a `project.godot` or `gdt.toml`; `--path <dir>` in the
Godot arguments picks another. `gdt current` prints the answer, and `gdt which` the executable.

If the chosen build is not installed, `gdt` prints the exact `gdt install ...` command and exits with 4, instead of
opening the project with some other version and upgrading its files.

## Pinning a project

```sh
gdt pin 4.7          # looks it up, stores the exact version in gdt.toml
```

```toml
[godot]
version = "4.7.2"    # or "4.7", "4.8-rc1", "4.7.2-mono"
```

## Running and opening

- `gdt open` starts the editor on the project (or Godot's project manager outside one) and returns at once.
- `gdt run script.gd args` runs a script headless; read the arguments with `OS.get_cmdline_user_args()`.
- `gdt run -- <godot arguments>` passes everything else to Godot, with its output and exit code unchanged.

On Windows, `run` uses the `_console` build, the only one that prints to a terminal.

Details: [version management spec](/specs/version-management/), [ADR 0005](/adr/0005-version-resolution/).
