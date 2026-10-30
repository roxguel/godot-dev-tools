# CLAUDE.md

Guidance for Claude Code and contributors.

## What this is

Godot Development Tools: a cross-platform Rust CLI (`gdt`) that manages Godot versions, addons and tests. Documentation lives in [`docs/`](docs/README.md); start with the vision, the roadmap and the ADRs. First consumer project: Project Cozy (`../project-cozy`).

## Conventions

- **Language:** everything in English (code, docs, commits). Commits: conventional commits with scope (`feat(versions): ...`, `docs(adr): ...`).
- **Docs first:** each doc declares its status at the top. Decisions go in `docs/adr/`. Update the specs when behavior changes.
- **Architecture:** logic in the library crate, no printing or process exit there; the CLI is a thin layer (ADR 0001).
- **Rust:** stable toolchain; `cargo fmt` and `cargo clippy -- -D warnings` clean; errors typed in the library (`thiserror`), `anyhow` only in the binaries.
- **Platforms:** Windows, Linux and macOS must compile and pass unit tests in CI. Do not use OS-specific code outside a platform module.
- **Network and filesystem** access behind traits so it can be tested without either.
- **Short options:** give an option a one-letter form when it is frequent and harmless, with one letter meaning one thing everywhere (rules in `docs/specs/cli.md`, "Short options"). Rare or destructive options stay long-only.
- **Trademark:** never use the Godot logo; keep the "not affiliated" notice in the README.
