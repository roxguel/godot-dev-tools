# ADR 0007: Commands grouped by area, with shortcuts for the engine

Status: Accepted (2026-10-30)

## Context

A flat command set makes `install` and `add` ambiguous: they could mean a Godot version or an addon. Typing `gdt add 4.0` should not silently look like installing a version.

## Decision

- Group commands by area: `engine` for builds, `addon` for addons, plus `auth`, `shim`, `config` and `cache`. Project actions (`run`, `open`, `test`, `sync`, `inspect`, `init`) stay at the top level.
- The most used engine commands (`install`, `uninstall`, `list`/`ls`, `current`, `use`, `pin`, `which`) are also **shortcuts** at the top level, because managing Godot versions is the purpose of the tool (`gdt use 4.7.2`, like `fnm use`).
- Addon commands are never shortcuts. A top-level word that belongs to another group fails with a hint ("did you mean `gdt addon add`?").
- `install` with no arguments installs what the project declares (the engine at top level, the lockfile under `addon`); with arguments it installs that. `gdt sync` does both for a fresh clone.
- Running `gdt` with no arguments prints help and exits 0, like other managers (`fnm`, `rustup`, `cargo`, `gh`). It does not launch the editor.
- No Godot build is bundled with the installer. The first `gdt install` makes its build the global default when there is none.
- `-mono` is a suffix of the version spec (`4.7.2-mono`); `--mono` is accepted as the same thing.

## Consequences

- Clear errors and a predictable vocabulary; the price is a slightly longer command for addons.
- Two spellings for engine commands (`gdt install` and `gdt engine install`) must be kept equivalent and documented once.
