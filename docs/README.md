# Documentation

Every document declares its status at the top: `Draft`, `Accepted`, `Implemented`, `Superseded`. Where a spec is only partly implemented it says so and marks the missing parts *(planned)*.

| Document | What it covers |
|---|---|
| [vision.md](vision.md) | Goals, non-goals, principles, personas |
| [roadmap.md](roadmap.md) | Milestones in order |
| [open-questions.md](open-questions.md) | Things still to research or decide |
| [specs/cli.md](specs/cli.md) | Command surface |
| [specs/version-management.md](specs/version-management.md) | Installing Godot builds, resolving which one to use, the shim |
| [specs/addons.md](specs/addons.md) | Addon sources, manifest and lockfile |
| [specs/testing.md](specs/testing.md) | Test runner |
| [specs/authentication.md](specs/authentication.md) | Asset Store API tokens and where they are kept |
| [adr/](adr/) | Architecture decision records |

## ADR index

| # | Decision | Status |
|---|---|---|
| [0001](adr/0001-rust-cargo-workspace.md) | Rust, as a Cargo workspace with a library and a CLI | Accepted |
| [0002](adr/0002-dual-license.md) | Dual license MIT OR Apache-2.0 | Accepted |
| [0003](adr/0003-single-cli-with-subcommands.md) | One binary, `gdt`, with subcommands | Accepted |
| [0004](adr/0004-storage-layout.md) | Per-user storage in the platform's standard directories | Accepted |
| [0005](adr/0005-version-resolution.md) | How the Godot version for a project is resolved | Accepted |
| [0006](adr/0006-shim-and-explicit-modes.md) | Two ways to invoke Godot: explicit `gdt run` and a `godot` shim | Accepted |
| [0009](adr/0009-multicall-binary.md) | One multicall binary; the `godot` shim is `gdt` under another name | Accepted |
| [0007](adr/0007-command-structure.md) | Commands grouped by area, with shortcuts for the engine | Accepted |
| [0008](adr/0008-credential-storage.md) | Credentials from the environment or the OS credential store | Accepted |
