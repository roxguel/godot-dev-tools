---
title: The godot shim and PATH
description: Make a plain `godot` command follow gdt, and what gdt does and does not do to your PATH.
sidebar:
  order: 2
---

`gdt run` works without any setup. If you also want a plain `godot` command (for editor integrations, scripts or muscle
memory) that picks the right build for the project, install the shim.

```sh
gdt shim install
gdt shim status
```

`gdt shim install` puts `godot` in `gdt`'s bin folder (`gdt paths` shows it) and tells you what to add to `PATH`. The
shim is `gdt` itself under another name ([ADR 0009](/adr/0009-multicall-binary/)): a symbolic link on Linux and macOS, a
copy on Windows.

## PATH

`gdt` changes `PATH` only when you ask:

```sh
gdt shim install --modify-path
gdt shim uninstall --modify-path
```

- **Windows:** puts the folder first on the user `Path` in the registry, keeping the value's type and every other
  entry exactly as written, after saving a backup in `gdt`'s config folder. Close Windows Terminal completely and open
  it again; terminals that are already open keep their old `Path`. The system `Path` is never touched.
- **Linux and macOS:** adds a block marked `# >>> gdt >>>` … `# <<< gdt <<<` to your shell profile that loads a small
  script adding the folder to `PATH`, as `rustup` does. Only that block is ever added or removed.

A `godot` earlier on `PATH` than the shim wins. `gdt shim status` shows whether the shim is installed and up to date,
whether its folder is on `PATH`, and which `godot` would actually run.

## What the shim does

- Passes every argument to Godot untouched, including `-v`, `-h` and `--version`.
- Chooses the build as [`gdt run` does](/guides/versions/), from `--path`, `GDT_VERSION`, `gdt.toml`, `project.godot` or
  the default. It never uses the network and never installs anything.
- Prints nothing on success; its own errors go to stderr prefixed `gdt:`, with exit code 4 when no build is selected or
  installed.
- Returns Godot's exit code unchanged.

```sh
godot --path ~/games/cozy --headless -s tools/check.gd    # from any folder
```

Details: [version management spec](/specs/version-management/), [ADR 0006](/adr/0006-shim-and-explicit-modes/).
