# ADR 0005: How the Godot version for a project is resolved

Status: Accepted (2026-10-29)

## Context

`project.godot` records `config/features` with only major.minor and the C# flag. Projects need a patch-level pin to be reproducible, but also should work with no extra file.

## Decision

Resolution order, first match wins: `--godot` flag, `GDT_VERSION` environment variable, `gdt.toml` `[godot] version`, `project.godot` features (newest installed matching stable build), global default.

The project root is found from the working directory, or from the project path given in the command (`gdt open <path>`) or in Godot's `--path` argument.

Pre-releases are never chosen implicitly.

Details in [version-management.md](../specs/version-management.md).

## Consequences

- Zero-config projects work with whatever matching build is installed; committing `gdt.toml` makes it exact.
- Reading `--path` from the arguments means the shim must parse a few Godot flags; it only looks for `--path`, and ignores everything else.
