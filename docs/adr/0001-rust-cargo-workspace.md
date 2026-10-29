# ADR 0001: Rust, as a Cargo workspace with a library and a CLI

Status: Accepted (2026-10-29)

## Context

The tool must be cross-platform (Windows, Linux, macOS), installable without a runtime, fast to start (it runs in front of every `godot` call when used as a shim), and reusable by a future GUI or editor plugin.

## Decision

Write it in Rust, as a Cargo workspace:

- `crates/godot-dev-tools`: library with all logic (versions, addons, tests, project parsing). No printing, no process exit; returns typed results and errors.
- `crates/gdt`: the CLI binary, a thin layer over the library (argument parsing, output, exit codes).
- `crates/gdt-shim`: the tiny `godot` shim binary, with minimal dependencies for fast startup.

## Consequences

- Static single binaries, easy cross-compilation and release artifacts per target.
- A GUI or plugin can depend on the library crate.
- Rust has a learning curve for the maintainer; the code stays simple and well documented to compensate.
- Crate names will be confirmed on crates.io before the first publish.
