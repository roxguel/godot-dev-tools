# Version management

Status: Draft. Release asset names are assumptions to verify (see [open-questions.md](../open-questions.md)).

## Version identifiers

A version is `MAJOR.MINOR[.PATCH]` plus an optional channel and a flavor:

| Text | Meaning |
|---|---|
| `4.7.1` | Stable 4.7.1, standard build |
| `4.7` | Latest installed (or latest available, when installing) stable 4.7.x |
| `4.7.0` | Exactly 4.7.0. Godot names it `4.7-stable` (no patch); `gdt` shows it as `4.7` |
| `4.7.1-mono` | The .NET build (`--mono` is the same as the suffix) |
| `4.8-rc1`, `4.8-beta2`, `4.8-dev3` | Pre-releases; never selected by a loose `4.8` while stable exists |
| `latest` | Latest stable |

Stable is the default channel. Pre-releases must be named explicitly.

## Sources

All builds come from GitHub releases:

- Stable: `godotengine/godot` releases.
- Pre-releases: `godotengine/godot-builds` (to verify).

The asset for the current OS, architecture and flavor is chosen from the release's asset list; the published checksum file is downloaded and the archive is verified before extraction. A mismatch aborts with exit code 5 and installs nothing. Downloads go to a cache and are extracted into a temporary folder, then moved into place so a failed install never leaves a half-installed version.

## Storage

Per ADR 0004, under the per-user data directory (`GDT_HOME` overrides it):

```
<data>/gdt/
  versions/<id>/          one folder per installed build, e.g. 4.7.1, 4.7.1-mono
  cache/downloads/        archives, safe to delete
  bin/                    shims and the gdt executable link
  config.toml             user configuration
```

## Resolution

Which build to use for a command, first match wins (ADR 0005):

1. `--godot <version>` flag.
2. `GDT_VERSION` environment variable.
3. `[godot] version` in the project's `gdt.toml`.
4. The project's `project.godot`: `config/features` holds `"MAJOR.MINOR"` and `"C#"` for .NET projects. This resolves to the newest installed matching stable build (and flavor).
5. The global default (`gdt use`).

If the result is not installed: `gdt run` / `gdt open` / the shim print the exact `gdt install ...` command and exit with code 4. An opt-in setting (`auto_install = true`) installs it on the spot.

### Finding the project

The project root is the nearest ancestor of the working directory containing `project.godot` or `gdt.toml`. When the Godot arguments include `--path <dir>` (or `--path=<dir>`), or a `project.godot` path is given to `gdt open`, that directory is used instead of the working directory. This is what lets `godot --path <worktree> -s tools/x.gd` work from any folder.

## `gdt.toml` (version part)

```toml
[godot]
version = "4.7.1"   # or "4.7", "4.8-rc1"
mono = false
```

## Opening and running

- `gdt open <path>`: resolve for the project, start the editor (`--editor --path <project>`), detach from the terminal, return immediately.
- `gdt run -- <args>`: resolve, run in the foreground in the current directory, forward stdin/stdout/stderr and the exit code.

## The shim

Two ways to invoke Godot (ADR 0006):

1. **Explicit:** `gdt run -- ...`. No setup beyond `gdt` itself.
2. **Shim:** `gdt shim install` places a small `godot` executable in `<data>/gdt/bin` and tells the user to put that folder on `PATH` (it never edits shell profiles or the registry unless asked with `--modify-path`). The shim resolves exactly as `gdt run` does and `exec`s the real Godot, forwarding arguments, environment and exit code. On Unix it uses `exec`; on Windows it is a separate tiny `.exe` that spawns and waits, since there are no unprivileged symlinks.

The shim never prints anything of its own on success, so scripts that parse Godot's output are unaffected. On failure it prints a message prefixed with `gdt:` to stderr.
