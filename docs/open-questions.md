# Open questions

Status: Draft. Remove an item when it is answered, and record the answer in a spec or an ADR.

## Research needed

1. **Release assets.** Exact asset names and checksum files for stable builds in `godotengine/godot` and for `rc` / `beta` / `dev` builds in `godotengine/godot-builds`, on each platform, for the standard and the .NET (Mono) flavors. The version spec assumes names like `Godot_v4.7.1-stable_win64.exe.zip`; verify before coding the resolver.
2. **Godot upgrade path.** What is official and scriptable: `--convert-3to4` (3 to 4 only), and for 4.x to 4.y, opening headless with `--import` / `--quit` to re-import. What breaks, and what a safe `gdt upgrade` must do (git cleanliness check, backup, dry run).
3. **Godot trademark policy, full text.** Only excerpts were reviewed. Read the whole policy at <https://godot.foundation/policies-and-procedures/trademark-policy> and ask the Foundation in writing if anything about the name is unclear.
4. **New Godot Asset Store API.** Is there a public, documented API for searching and downloading? The legacy Asset Library has a REST API.
5. **Test runners' command lines.** GUT and gdUnit4 headless entry points and report formats, per version.
6. **Windows shim.** A small separate `.exe` shim versus a `.cmd` file; behavior when stdin/stdout are attached to a console, signal and exit code forwarding.
7. **.NET flavor.** Needs the .NET SDK on the machine; decide what `gdt doctor` checks and what `gdt install` says about it.

## Decisions pending

1. Name of the project file: `gdt.toml` is assumed. Confirm, and confirm whether a plain `.godot-version` file should also be read.
2. Whether `gdt open` should refuse to open a project with a lower engine version than the one that last saved it.
3. Crate name reservation on crates.io (`godot-dev-tools`), and when.
4. Telemetry: none. Confirm as a permanent rule.
