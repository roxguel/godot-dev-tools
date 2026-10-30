# Version management

Status: Draft. Release asset names verified against GitHub on 2026-10-29 for Godot 4.x (see Sources).

## Version identifiers

A version is `MAJOR.MINOR[.PATCH]` plus an optional channel and a flavor:

| Text | Meaning |
|---|---|
| `4.7.1` | Stable 4.7.1, standard build |
| `4` | The newest stable release of that major version. Redundant while 4 is the newest major, and it keeps working when a newer one appears |
| `4.7` | Latest installed (or latest available, when installing) stable 4.7.x |
| `4.7.0` | Exactly 4.7.0. Godot names it `4.7-stable` (no patch); `gdt` shows it as `4.7` |
| `4.7.1-mono` | The .NET build. The suffix is canonical; `--mono` on a command means the same |
| `beta`, `rc`, `dev` | The newest published build of that channel (for example `4.8-beta2`) |
| `pre` | The newest pre-release of any channel |
| `4.8-rc1`, `4.8-beta2`, `4.8-dev3` | Pre-releases; never selected by a loose `4.8` while stable exists |
| `latest` | Latest stable |

Stable is the default channel. Pre-releases must be named explicitly, either exactly (`4.8-rc1`) or with a channel alias (`beta`, `rc`, `dev`, `pre`).

Aliases (`latest`, `beta`, `rc`, `dev`, `pre`) are resolved when the command runs. `gdt use` and `gdt pin` store the **concrete** version they resolved to, never the alias, so a pinned project stays reproducible.

No Godot build comes with `gdt` itself. The first `gdt install` makes its build the global default when none is set.

## Sources

All builds come from GitHub releases:

- **Stable:** `godotengine/godot`, tag `<version>-stable` (for example `4.7.2-stable`).
- **Pre-releases:** `godotengine/godot-builds`, tag `<version>-<channel>` (for example `4.8-beta2`, `4.8-dev7`). That repository also mirrors the stable releases, but `gdt` reads stable from the main repository.

### Asset names (Godot 4.x)

`V` is the tag (`4.7.2-stable`, `4.8-beta2`). Verified for 4.7.2-stable; pre-release names follow the same shape.

| Platform | Standard | .NET (`mono`) |
|---|---|---|
| Windows x86_64 | `Godot_vV_win64.exe.zip` | `Godot_vV_mono_win64.zip` |
| Windows arm64 | `Godot_vV_windows_arm64.exe.zip` | `Godot_vV_mono_windows_arm64.zip` |
| Linux x86_64 | `Godot_vV_linux.x86_64.zip` | `Godot_vV_mono_linux_x86_64.zip` |
| Linux arm64 | `Godot_vV_linux.arm64.zip` | `Godot_vV_mono_linux_arm64.zip` |
| macOS (universal) | `Godot_vV_macos.universal.zip` | `Godot_vV_mono_macos.universal.zip` |

Things the resolver must not assume:

- The separators differ between flavors (`linux.x86_64` versus `mono_linux_x86_64`; `.exe.zip` versus `.zip`), so names are built from an explicit table per platform and flavor, not from a pattern.
- Each release has many unrelated assets (Android, web, export templates, debug symbols, headless/server variants in 3.x). Only exact names from the table are accepted.
- Godot 3.x uses different names (`x11_64`, `linux_headless`). Godot 3 is out of scope for now.
- A pre-release may lack some platforms (a given beta did not list a macOS build in the check made); `gdt` reports "no build for your platform in this release" instead of failing obscurely.

### Checksums

Each release publishes one `SHA512-SUMS.txt` asset with lines of the form `<sha512 hex>  <asset name>` (two spaces). The file is downloaded from the same release and the archive's SHA-512 must match its line before extraction. A mismatch, or a missing line, aborts with exit code 5 and installs nothing. (The `.sha256` file on a release is only for the source tarball and is not used.) Downloads go to a cache and are extracted into a temporary folder, then moved into place so a failed install never leaves a half-installed version.

Note: the checksum file is hosted next to the archive, so it protects against corrupt or tampered downloads but not against a compromised release. Signature verification is not offered by Godot at the time of writing.

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
version = "4.7.1"        # or "4.7", "4.8-rc1", "4.7.1-mono" for the .NET build
```

There is a single `version` field; the flavor is the `-mono` suffix. The project file is `gdt.toml` only (no JSON variant), so there is never a conflict between two manifests.

## Opening and running

- On Windows a build has two executables: `Godot_v..._win64.exe` (the editor, a GUI program) and `Godot_v..._win64_console.exe` (keeps a terminal attached). Output only reaches a terminal from the console one, so `gdt run` and the shim use the console executable and `gdt open` uses the editor one. Verified on 4.7.2, whose zip holds just these two files at the root; the Linux, macOS and .NET layouts are still to verify.
- `gdt open <path>`: resolve for the project, start the editor (`--editor --path <project>`), detach from the terminal, return immediately. Outside a project, or with `--manager`, it starts Godot's project manager with the global default build.
- `gdt run -- <args>`: resolve, run in the foreground in the current directory, forward stdin/stdout/stderr and the exit code. `gdt run script.gd` is the shortcut for `--headless -s script.gd` (the script must extend `SceneTree` or `MainLoop`).
- `gdt engine info`: the executable, the version, the flavor, the editor settings folder (per user, shared by builds of the same major version, to verify), the project's `.godot/` folder, and whether the build is self-contained (a `_sc_` file next to the executable makes Godot keep its data beside it; to verify).

## The shim

Two ways to invoke Godot (ADR 0006):

1. **Explicit:** `gdt run -- ...`. No setup beyond `gdt` itself.
2. **Shim:** `gdt shim install` puts a `godot` in `<data>/bin` and says what to add to `PATH` (it never edits `PATH`, shell profiles or the registry). The shim is `gdt` itself started under the name `godot` (ADR 0009): a symbolic link on Unix, a copy on Windows.

In shim mode:

- Every argument goes to Godot untouched, including `-v`, `-h` and `--version`, which are Godot's. Nothing is parsed by `gdt`.
- The build is resolved as for `gdt run` (project from `--path` or the current folder, `GDT_VERSION`, `gdt.toml`, `project.godot`, the default); there is no `--godot` flag. It never uses the network and never installs anything.
- Nothing is printed on success. A failure prints `gdt: <message>` on stderr and exits with 4 (build not installed or none selected), 2 (bad `GDT_VERSION`) or 1.
- Godot's exit code is returned unchanged, as a full 32-bit value. On Unix the shim replaces itself with Godot; on Windows it runs the console executable and ignores Ctrl+C, which Godot receives too, so that Godot ends first.
- It refuses to run if the build's executable turns out to be the shim itself.

`gdt shim status` prints whether the shim is installed and up to date, whether its folder is on `PATH` and at which entry, and which `godot` would run (checking `PATHEXT` on Windows). A `godot` earlier on `PATH` than the shim wins, and on Windows the system `Path` comes before the user `Path`, so `status` is the thing to check. A newer `gdt` leaves the Windows copy old until `gdt shim install` is run again; while a Godot started through the shim is open, Windows will not let the file be replaced, and `gdt` says so.

Why a plain `godot --version` follows `gdt use`: only with the shim installed and its folder first on `PATH`. Without it, `gdt run -- --version` shows the same.
