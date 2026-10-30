# ADR 0009: One multicall binary; the `godot` shim is `gdt` under another name

Status: Accepted (2026-10-31). Supersedes the separate `gdt-shim` crate of [ADR 0001](0001-rust-cargo-workspace.md) and the "own small binary" of [ADR 0006](0006-shim-and-explicit-modes.md).

## Context

ADR 0006 planned the `godot` shim as its own tiny binary so that its startup cost stays low, since it sits in front of every Godot call. In practice that needs a second executable to build, ship and keep in step with `gdt`. `cargo install --path crates/gdt` installs one binary only, so `gdt shim install` would have nothing to put in `bin/`.

## Decision

`gdt` is a **multicall binary**, like `rustup`. If its `argv[0]` has the file stem `godot` it runs in shim mode and does nothing but choose a Godot build and run it. `gdt shim install` puts a copy of `gdt` named `godot` in `<data>/bin` (a symbolic link on Unix, so it never goes stale; a copy on Windows, where links need privileges).

Shim mode:

- is decided before argument parsing, so every argument (including Godot's own `-v`, `-h` and `--version`) reaches Godot untouched;
- prints nothing on success; its own errors go to stderr prefixed `gdt:`;
- resolves the build exactly as `gdt run` does (ADR 0005), without the `--godot` flag but with `GDT_VERSION`, and never touches the network;
- on Unix replaces itself with Godot (`exec`); on Windows spawns the console executable, ignores Ctrl+C so that Godot (which receives it too) ends first, and returns Godot's exit code as a full `i32`;
- refuses to run itself if the resolved executable is the shim.

The separate `gdt-shim` crate is removed. `gdt shim status` reports whether the copy is up to date (a `cargo install` of a newer `gdt` leaves the Windows copy old until `gdt shim install` is run again), whether the bin folder is on `PATH`, and which `godot` would actually run.

`gdt` does not change `PATH` by itself. It prints what to add. `--modify-path` is left out until the user has agreed how to edit the Windows user `Path` without flattening its `REG_EXPAND_SZ` entries.

`godot-mono` is not provided: the build follows the resolution (a C# project picks the .NET build, and `GDT_VERSION=4.7.2-mono` forces it).

## Measured cost

On Windows 11, a 4.1 MB release `gdt` as `godot.exe` against the real 4.7.2 console executable, `--version`, 20 runs per round, three rounds: about 116 to 158 ms through the shim against 94 to 96 ms direct, an overhead of **22 to 62 ms** (noisy; Windows Defender scanning a freshly copied executable is the likeliest part of it). A minimal separate shim was not built for comparison. If the overhead ever matters, the shim path can be made lighter without changing this decision, for example by keeping `gdt`'s start-up free of work that shim mode does not need.

## Consequences

- One binary to install, update and ship (the vision's "single static binary").
- A heavier executable is started for every `godot` call than a minimal shim would be.
- On Windows the shim file is locked while a Godot started through it runs, so refreshing it then fails; the error says to close those windows first.
