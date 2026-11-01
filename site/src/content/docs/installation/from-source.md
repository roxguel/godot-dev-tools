---
title: Build from source
description: Build and install gdt with a Rust toolchain.
---

You need a [Rust toolchain](https://rustup.rs), version 1.85 or newer.

The quickest way is [crates.io](https://crates.io/crates/gdt), which builds the released version:

```sh
cargo install gdt --locked
```

To build the repository instead:

```sh
git clone https://github.com/roxguel/godot-dev-tools
cd godot-dev-tools
cargo install --path crates/gdt --locked
```

This puts `gdt` in Cargo's bin folder (`~/.cargo/bin`, which `rustup` already put on your `PATH`). `--locked` builds
with the dependency versions recorded in `Cargo.lock`, the ones CI tests.

To work on `gdt` itself, the checks that must pass before a pull request are in
[CONTRIBUTING](/project/contributing/):

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

To update, pull and run `cargo install` again; `gdt self-update` is *(planned)*.
