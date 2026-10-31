# Changelog

All notable changes to this project are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, a minor version can change the command line or the project files; the changes are listed here.

## [0.1.0] - Unreleased

First release: a version manager, a `godot` shim, an addon manager and a test runner for Godot 4, in one command, `gdt`.

### Added

**Godot versions**
- `gdt install`, `uninstall`, `list` (`ls`), `current`, `use`, `pin` and `which`, also under `gdt engine ...`. Builds come from the official GitHub releases (`godotengine/godot` for stable, `godotengine/godot-builds` for pre-releases) and are checked against the published SHA-512 sums before they are installed.
- Version specs: `4`, `4.7`, `4.7.2`, `4.8-rc1`, the aliases `latest`, `beta`, `rc`, `dev` and `pre`, and the `-mono` suffix (or `--mono`) for the .NET build. A pre-release is never picked by a loose spec.
- The version a project uses is resolved, with no silent fallback, from `--godot`, `GDT_VERSION`, `[godot] version` in `gdt.toml`, the engine in `project.godot` (a C# project picks the .NET build), and the global default. A build that is not installed is reported with the exact `gdt install` command and exit code 4.
- `gdt run` runs Godot with the resolved version from any folder: `gdt run script.gd args` for a headless script, or `gdt run -- <godot arguments>`, with Godot's output and exit code passed through. `--path` decides which project's version is used.
- `gdt open` opens the editor on a project, or Godot's project manager outside one. `gdt inspect` shows what engine a project asks for. `gdt sync` makes a fresh clone ready: the engine, then the addons.

**The `godot` shim**
- `gdt shim install`, `uninstall` and `status` put a `godot` on your `PATH` that follows gdt: `godot --path <project> ...` uses the project's version from any folder. It is `gdt` itself under another name, passes every argument and the exit code through unchanged, and never uses the network.
- `gdt` changes `PATH` only when asked, with `--modify-path` (the user `Path` in the registry on Windows, with a backup; a marked block in the shell profile on Linux and macOS).

**Addons**
- `gdt addon add`, `install`, `remove` (`rm`), `list` (`ls`), `update` and `search`, with three sources: GitHub (`github:owner/repo@ref`, a tag, a branch or a commit), the Godot Asset Store (`publisher/asset`) and the legacy Asset Library (`library:123`).
- Addons are declared in `gdt.toml` and pinned in `gdt.lock` with the exact commit or release and a hash of the installed files. `gdt addon install` reproduces the lock and refuses to overwrite a folder you changed unless `--force` is given. `--locked` fails instead of changing the lock.
- `dev = true` (`--dev`) marks addons that are needed to work on the project but not shipped; `--no-dev` skips them.
- By default the folders gdt installed are listed in a managed block of `.gitignore`; `[addons] git = "track"` or `"ignore"` change that.
- Paid assets are never bought: `gdt` says where to get them and exits with 6.

**Tests**
- `gdt test` runs the project's tests headless with the right Godot: GUT, gdUnit4 or a script of your own, found from the installed addons or `[test]` in `gdt.toml`. It imports a fresh clone first, judges the result from the JUnit report and not from the exit code alone, and exits with 10 when tests fail. `--report` copies the JUnit file.

**General**
- `gdt paths` shows where gdt keeps its files; `GDT_HOME` moves all of them.
- `-j`/`--json` on commands that print data, `-q` for bare ids, `-v`/`-vv` for diagnostics on stderr, `--progress` and `--color`. Data goes to stdout and messages to stderr.
- Stable exit codes: 0 success, 1 failure, 2 usage, 3 network, 4 version not found or not installed, 5 checksum mismatch, 6 paid asset, 10 tests failed.
- `GITHUB_TOKEN` or `GH_TOKEN` raises the GitHub rate limit and reaches private repositories. It is sent only to `api.github.com` and is never printed.
- Release binaries for Windows, Linux and macOS (x86_64 and arm64; the Linux ones are static), each with a `SHA256SUMS` file and a build provenance attestation, and the installers `install.sh` and `install.ps1`.

### Security

- Addon names and folders that come from `gdt.toml`, `gdt.lock` or the command line are validated, so none of them can read, write or delete outside `addons/`. A key that gdt does not know in `gdt.toml` is an error. Names may only have letters, digits, spaces, `.`, `_` and `-`.
- Addon archives cannot contain symbolic links, and no archive entry is written through a link made by an earlier entry, so an archive cannot write outside the folder it is unpacked into.
- gdt does not follow symbolic links for `addons/`, an addon folder, `gdt.toml`, `gdt.lock` or `.gitignore`, so a cloned project cannot make it read, write or delete elsewhere.
- `-v` and `-vv` mask `Authorization` headers, GitHub tokens and the value of `GH_TOKEN`/`GITHUB_TOKEN` before printing.
- Downloads are verified against published checksums before use; a cached release list that cannot be read is discarded.

### Known limitations

- Tested by hand on Windows. The Linux and macOS builds are compiled and unit-tested in CI but not yet exercised by hand.
- gdUnit4 support follows its documentation and has not been run against a real project.
- The Windows binaries are not code-signed, so SmartScreen can warn on the first run.
- Not available yet: addon sources by URL or local path, signing in to the Asset Store (paid assets), `gdt init`, `doctor`, `config`, `cache`, `completions`, `self-update` and `gdt upgrade`. See [the roadmap](docs/roadmap.md).

[0.1.0]: https://github.com/roxguel/godot-dev-tools/releases/tag/v0.1.0
