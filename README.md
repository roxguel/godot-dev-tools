# Godot Development Tools

> **Status: pre-release.** The version manager, the `godot` shim, the addon manager and the test runner work today and are tested on Windows. No release has been published yet: build it from source (below). Linux and macOS are compiled and unit-tested in CI but not yet exercised by hand.

One cross-platform tool for working in a Godot ecosystem. The command is `gdt`.

- **Version manager** (think `fnm` / `rustup` for Godot): install any official Godot build, pick which one to use globally or per project, and run the right one from any folder.
- **Addon manager** (think `npm` for Godot): install addons from GitHub, the Godot Asset Store or the legacy Asset Library, pinned by a lockfile.
- **Test runner**: run GUT, gdUnit4 or plain script suites headless, with CI-friendly exit codes and JUnit XML.

## Why

Opening a project with the wrong Godot version silently upgrades its files. Running Godot from a script means hard-coding an executable path. Sharing addons means copying folders around. `gdt` makes the version, the addons and the test command part of the project, so a fresh clone works the same on every machine.

## Install

There are no binaries to download yet. With a [Rust toolchain](https://rustup.rs) (1.85 or newer):

```bash
git clone https://github.com/roxguel/godot-dev-tools
cd godot-dev-tools
cargo install --path crates/gdt --locked
```

This puts `gdt` in Cargo's bin folder. Release binaries for Windows, Linux and macOS (x86_64 and arm64), with checksums, will be attached to GitHub releases.

## A taste

```bash
gdt install 4.7.2                      # download an official build and verify its checksum
gdt pin 4.7                            # write the build into gdt.toml, in the current project
gdt open                               # open the editor with the version the project asks for
gdt run tools/foo.gd one two           # run a script headless with that version, from any folder
gdt run -- --path . --import           # or pass any arguments to Godot
gdt addon add github:bitwes/Gut@v9.7.1 --dev   # install an addon and lock it
gdt test                               # run the project's tests headless (exit 10 if any fail)
gdt sync                               # on a fresh clone: the engine, then the addons
gdt shim install                       # make a plain `godot` follow gdt
```

Outside a project, `gdt` uses the global default (`gdt use 4.7.2`; the first build you install becomes the default). Inside one, the first match of these decides, and there is no silent fallback:

1. `--godot <version>`
2. the `GDT_VERSION` environment variable
3. `[godot] version` in `gdt.toml`
4. the engine in `project.godot` (`config/features`; a C# project picks the .NET build)
5. the global default

If the chosen build is not installed, `gdt` prints the exact `gdt install ...` command and exits with 4.

## Commands

| Area | Commands |
|---|---|
| Engine builds | `install`, `uninstall`, `list` (`ls`), `current`, `use`, `pin`, `which`, also under `gdt engine ...` |
| Project | `run`, `open`, `inspect`, `sync`, `test` |
| Addons | `gdt addon add`, `install`, `remove` (`rm`), `list` (`ls`), `update`, `search` |
| System | `shim install`, `shim uninstall`, `shim status`, `paths` |

`gdt --help` and `gdt <command> --help` list every option. Commands that print data take `-j`/`--json`; data goes to stdout and messages to stderr. The full surface, including what is still planned (`init`, `doctor`, `self-update`, `config`, `cache`, `completions`, `auth`), is in [`docs/specs/cli.md`](docs/specs/cli.md).

## Project files

`gdt.toml` and `gdt.lock` live in the project and are committed:

```toml
[godot]
version = "4.7.2"                  # or "4.7", "4.8-rc1", "4.7.2-mono" for the .NET build

[addons.gut]
github = "bitwes/Gut"
ref = "v9.7.1"                     # a tag, a branch or a commit
dev = true                         # needed to work on the project, not shipped

[test]
runner = "gut"                     # found from the installed addons when absent
paths = ["test"]
```

`gdt.lock` records the exact commit or release and a hash of the installed files for every addon, and `gdt addon install` reproduces it. By default `addons/` stays in Git except for the folders `gdt` installed, which it adds to a managed block in `.gitignore` (`[addons] git = "track"` or `"ignore"` change that). Keys `gdt` does not know are errors, so a typo is never ignored silently. See [`docs/specs/addons.md`](docs/specs/addons.md).

## Configuration

| Variable | Effect |
|---|---|
| `GDT_HOME` | Keep everything `gdt` stores in this folder instead of the platform's standard directories (`gdt paths` shows where). It must be an absolute path: a relative one, such as `~/.gdt` in quotes, is an error |
| `GDT_VERSION` | Use this installed build for the command, over the project |
| `GITHUB_TOKEN` or `GH_TOKEN` | Raise the GitHub API rate limit and reach private repositories. Sent only to `api.github.com` and never printed |
| `NO_COLOR` | No colors, as `--color never` |

## Exit codes

`0` success, `1` failure, `2` usage error, `3` network, `4` version not found or not installed, `5` checksum or hash mismatch, `6` paid asset (`gdt` never buys), `10` tests failed. `gdt run` and the shim pass Godot's own exit code through unchanged.

## Documentation

The documentation site is at <https://gdt.roxguel.dev> (its source is in [`site/`](site/README.md)). The design lives in [`docs/`](docs/README.md): the vision, the roadmap, the specs of each area and the architecture decision records; the site publishes them.

## AI-assisted development

This project is developed with the help of AI coding assistants (Claude Code). Every change goes through a pull request that the maintainer reviews and merges, and the design decisions are recorded in [`docs/`](docs/README.md).

## Names

| What | Name |
|---|---|
| Project | Godot Development Tools |
| Repository and crate | `godot-dev-tools` |
| Command | `gdt` |

## Platforms

Windows, Linux and macOS (x86_64 and arm64) are the targets. Development and manual testing happen on Windows; Linux and macOS builds and unit tests come from CI.

## Not affiliated

This is an independent community project. It is not affiliated with, endorsed by or sponsored by the Godot Foundation or the Godot Engine project. "Godot" is a trademark of the Godot Foundation; it is used here under its [trademark policy](https://godot.foundation/policies-and-procedures/trademark-policy) to describe what the tool works with. This project does not use the Godot logo.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
