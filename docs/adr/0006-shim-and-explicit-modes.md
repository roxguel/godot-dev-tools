# ADR 0006: Two ways to invoke Godot: explicit `gdt run` and a `godot` shim

Status: Accepted (2026-10-29)

## Context

Scripts, editors, MCP servers and CI call `godot` directly. A tool that only offers `gdt run` forces every caller to change; a tool that only offers a shim hides what is happening.

## Decision

Support both:

- **Explicit:** `gdt run -- <args>` and `gdt open`. Visible, no `PATH` change.
- **Shim:** `gdt shim install` provides `godot` (and on request `godot-mono`) in `<data>/gdt/bin`. It resolves the version exactly as `gdt run` does.

The shim is its own small binary (ADR 0001), forwards arguments, environment, stdio and exit code, and does not edit `PATH` by itself.

Implementation order: explicit first (M1), shim second (M2).

## Consequences

- Existing commands such as `godot --path <worktree> -s tools/x.gd` work unchanged from any folder.
- The shim sits in the hot path of every Godot call, so its startup cost and dependencies are kept minimal.
