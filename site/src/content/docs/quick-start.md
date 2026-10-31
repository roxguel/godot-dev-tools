---
title: Quick start
description: From nothing to a project that opens, runs and tests with the right Godot, in a few commands.
---

After [installing gdt](/installation/).

## 1. Get a Godot build

```sh
gdt install 4.7.2        # downloads the official build and checks its SHA-512
gdt ls                   # what you have; `gdt ls -r` shows what you can install
```

The first build you install becomes the global default (`gdt use 4.7.2` changes it). Add `-mono` for the .NET build:
`gdt install 4.7.2-mono`.

## 2. Tie a project to it

In the project folder:

```sh
gdt pin 4.7              # writes [godot] version = "4.7.x" into gdt.toml
gdt open                 # the editor, with the version the project asks for
gdt run tools/foo.gd     # a script, headless, from any folder
gdt run -- --path . --import
```

Commit `gdt.toml`. Anyone who clones the project gets the same version. If that build is not installed, `gdt` prints
the exact `gdt install ...` command and exits with 4; `gdt sync` does it for you.

## 3. Add addons and tests

```sh
gdt addon add github:bitwes/Gut@v9.7.1 --dev    # install GUT, record it, pin it in gdt.lock
gdt test                                         # run the tests headless; exit 10 if any fail
```

Commit `gdt.toml` and `gdt.lock`.

## 4. On a fresh clone

```sh
gdt sync                 # the engine, then the addons
gdt test
```

## 5. Optional: a `godot` that follows `gdt`

```sh
gdt shim install --modify-path
```

Then `godot --path <project> ...` uses the project's version, from any folder. See [the shim](/guides/shim/).

## Where next

- [Versions and how one is chosen](/guides/versions/)
- [Addons, `gdt.toml` and `gdt.lock`](/guides/addons/)
- [Tests in CI](/guides/tests-in-ci/)
- [Every command](/reference/cli/)
