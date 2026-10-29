# ADR 0004: Per-user storage in the platform's standard directories

Status: Accepted (2026-10-29)

## Decision

Everything `gdt` installs lives in the user's standard data directory, never in system locations, so no administrator rights are needed:

| OS | Data directory | Config directory |
|---|---|---|
| Windows | `%LOCALAPPDATA%\gdt` | `%APPDATA%\gdt` |
| Linux | `$XDG_DATA_HOME/gdt` (default `~/.local/share/gdt`) | `$XDG_CONFIG_HOME/gdt` (default `~/.config/gdt`) |
| macOS | `~/Library/Application Support/gdt` | same |

The environment variable `GDT_HOME` replaces both with a single root, and `config.toml` can set `versions_dir` to relocate installed builds. Changing the location is supported but not a focus of the first milestones.

Layout inside the data directory is described in [version-management.md](../specs/version-management.md).

## Consequences

- Predictable, uninstallable by deleting one folder.
- A developer with an existing folder of Godot builds can point `versions_dir` at it, if names match; adopting existing folders is a later convenience.
