# Roadmap

Status: Draft. Updated 2026-11-01.

Order matters: the first milestones remove the day-to-day pain in the first consumer project (Project Cozy) and each later one builds on them.

| Milestone | Scope | Done when |
|---|---|---|
| **M0 Foundations** (implemented) | Docs, ADRs, licenses, Cargo workspace skeleton, CI that builds and tests on Windows, Linux and macOS | `cargo build` and `cargo test` pass in CI on the three platforms |
| **M1 Versions** (implemented) | `install`, `uninstall`, `list`, `use`, `pin`, `which`, `run`, `open`; GitHub release discovery; checksum verification; version resolution | A fresh machine can `gdt install 4.7.1 && gdt run -- --version` |
| **M2 Shim** (implemented) | `godot` shim (multicall, ADR 0009), `gdt shim install/uninstall/status`, `PATH` guidance, `--path` awareness, `--modify-path` | `godot --path <project> -s x.gd` run from an unrelated folder uses the project's version |
| **M3 Addons (git)** (implemented) | `gdt.toml` addons table, `add`, `remove`, `install`, `update`, `gdt.lock`, git source with subdirectory selection | A project with addons restores identically from a clean clone |
| **M4 Addons (stores)** (implemented, except URL sources and signing in) | Asset Library (legacy) source, new Asset Store source, URL/archive source, `search` | Same lockfile semantics as git sources |
| **M5 Tests** (implemented: GUT and scripts; gdUnit4 not run yet) | `gdt test`, detection of GUT / gdUnit4 / plain script suites, JUnit XML, exit codes | CI example in the docs passes on a clean runner |
| **M6 First release** (0.1.0, 2026-11-01) | Full review of the repository and a security audit, `CHANGELOG`, version 0.1.0, release binaries for the six targets with `SHA256SUMS` and build attestations (the release workflow exists), one-line installers, the documentation site (gdt.roxguel.dev) | A tag `v0.1.0` produces a GitHub release that installs on a clean machine |
| **M7 Distribution** | Winget, Scoop, Homebrew and crates.io; `gdt self-update` | `winget install` and `brew install` give a working `gdt` |
| **M8 Upgrade** | `gdt upgrade` to move a project to another engine version using Godot's own conversion and re-import | Researched in [open-questions.md](open-questions.md) first |
| **Later** | `gdt init`, `doctor`, `config`, `cache`, `completions`, `auth` (Store sign-in), `engine info`, URL and path addon sources, export templates, GUI, editor plugin | |

macOS is a target from M0 (compiled and unit-tested in CI) but is not exercised manually until someone with a Mac can.
