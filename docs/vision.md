# Vision

Status: Draft

## Problem

Working with Godot across several projects and machines means juggling things by hand:

- Several engine versions installed in ad-hoc folders, launched with hard-coded paths.
- No record in the project of which build it needs (only `config/features` in `project.godot`, which holds major.minor and the C# flag, never the patch).
- Addons installed by copying folders, with no lock and no update path.
- Test runners that differ per addon and need glue to run in CI.

## Goals

1. Install, list, remove and select official Godot builds, per user, per project or per command.
2. Make "which Godot does this project use?" a property of the project, resolved automatically.
3. Install addons from multiple sources reproducibly, with a lockfile.
4. Run tests headless with predictable exit codes and machine-readable reports.
5. Single static binary, no runtime to install, same behavior on Windows, Linux and macOS.
6. Scriptable first: every command works without prompts, supports `--json` where it prints data, and uses stable exit codes. A GUI or an editor plugin can sit on top of the same library later.

## Non-goals (for now)

- Building Godot from source or managing custom engine builds beyond "use this executable".
- Export templates, beyond a possible later `gdt templates` command.
- Replacing the editor, the Asset Library website, or a package registry. `gdt` consumes existing sources; it does not host anything.
- Dependency resolution between addons (addons are flat and pinned).

## Principles

- **Official sources only by default.** Engine builds come from the official GitHub releases, verified with the published checksums.
- **Project-local truth.** Anything that affects reproducibility lives in files committed with the project (`gdt.toml`, `gdt.lock`).
- **No surprises.** Never modify a project or install anything outside the `gdt` directories without a command that says so.
- **Library first.** All logic lives in a library crate; the CLI is a thin layer.

## Personas

- **Solo developer** with several projects on different Godot versions who wants `gdt open` and `godot` to just do the right thing.
- **Tooling and CI author** who needs `gdt install && gdt test` to work on a clean runner.
- **Addon user** who wants `gdt add` / `gdt install` instead of copying folders.
- **Contributor** to Godot projects who wants to run the same engine version as the maintainers.

## Naming and trademark

The project is "Godot Development Tools": the word Godot plus words that describe its purpose, as the Godot Foundation's trademark policy asks. It does not use the Godot logo and states that it is not affiliated. See the README.
