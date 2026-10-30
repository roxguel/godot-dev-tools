# Godot Development Tools

> **Status: pre-alpha, documentation phase.** Nothing is implemented yet. The design lives in [`docs/`](docs/README.md).

One cross-platform tool for working in a Godot ecosystem. The command is `gdt`.

- **Version manager** (think `fnm` / `rustup` for Godot): install any official Godot build, pick which one to use globally or per project, and run the right one from any folder.
- **Addon manager** (think `npm` for Godot): install addons from git repositories, the Godot Asset Library (legacy and new store), or plain URLs, pinned by a lockfile.
- **Test runner**: run GUT, gdUnit4 or plain script suites headless, with CI-friendly exit codes and JUnit XML.

## Why

Opening a project with the wrong Godot version silently upgrades its files. Running Godot from a script means hard-coding an executable path. Sharing addons means copying folders around. `gdt` makes the version, the addons and the test command part of the project, so a fresh clone works the same on every machine.

## A taste (planned)

```bash
gdt install 4.7.1                  # download an official build from GitHub
gdt pin 4.7                        # write gdt.toml in the current project
gdt open path/to/project.godot     # open the editor with the version the project asks for
gdt run -- --path . -s tools/foo.gd   # run Godot with that version, from any folder
gdt add github:owner/some-addon    # install an addon and lock it
gdt test                           # run the project's test suites headless
```

`gdt` can also install a `godot` shim on your `PATH`, so a plain `godot --path <project> ...` picks the right version by itself.

## AI-assisted development

This project is developed with the help of AI coding assistants (Claude Code). Every change goes through a pull request that the maintainer reviews and merges, and the design decisions are recorded in [`docs/`](docs/README.md).

## Names

| What | Name |
|---|---|
| Project | Godot Development Tools |
| Repository and crate | `godot-dev-tools` |
| Command | `gdt` |

## Platforms

Windows, Linux and macOS (x86_64 and arm64) are the targets. Development and testing start on Windows and Linux; macOS builds come from CI.

## Not affiliated

This is an independent community project. It is not affiliated with, endorsed by or sponsored by the Godot Foundation or the Godot Engine project. "Godot" is a trademark of the Godot Foundation; it is used here under its [trademark policy](https://godot.foundation/policies-and-procedures/trademark-policy) to describe what the tool works with. This project does not use the Godot logo.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this project, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
