# ADR 0003: One binary, `gdt`, with subcommands

Status: Accepted (2026-10-29)

## Context

The needs are three: version management, addon management, tests. They could be separate tools.

## Decision

A single CLI, `gdt`, with subcommands, like `npm` or `cargo`. The three areas share project discovery, configuration (`gdt.toml`), version resolution and the download machinery.

The name: project "Godot Development Tools", repository and crate `godot-dev-tools`, command `gdt`. `gdt` is short and free on crates.io; it is also the acronym of unrelated things (x86 Global Descriptor Table, geometric tolerancing), which does not matter in a Godot context but is the reason the repository and crate carry the descriptive name.

## Consequences

- One install, one config file, one place to learn.
- The binary grows over time; the library crate keeps areas separated by module.
