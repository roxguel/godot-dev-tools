# Contributing

Thanks for your interest. The project is young: the core works, and the most useful contributions right now are bug reports, testing on Linux and macOS, feedback on the documents in [`docs/`](docs/README.md) and answers to the items in [`docs/open-questions.md`](docs/open-questions.md).

## Workflow

- Open an issue before a large change so the approach can be agreed first.
- Work on a branch and open a pull request against `main`.
- Commits follow conventional commits with a scope: `feat(versions): ...`, `fix(addons): ...`, `docs(adr): ...`.
- Decisions that shape the architecture are recorded as ADRs in `docs/adr/`; behavior changes update the matching spec.

## Before sending a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

Code must compile and pass tests on Windows, Linux and macOS; CI checks all three. Keep OS-specific code in a platform module, and keep filesystem and network access behind traits so logic can be tested without them. A name or path that comes from outside (the command line, `gdt.toml`, `gdt.lock`, an archive) must go through `crates/godot-dev-tools/src/names.rs` before it is used to build a path, and a change that touches such code needs a regression test with a hostile input.

## The documentation site

`site/` is an [Astro Starlight](https://starlight.astro.build) site published at <https://gdt.roxguel.dev>. It reuses `docs/`, `CHANGELOG.md`, `SECURITY.md` and this file instead of copying them (`site/scripts/sync-docs.mjs` generates git-ignored pages from them), so edit those, not generated pages. The installers live in `site/public/install.sh` and `install.ps1`; their URLs (`/install.sh`, `/install.ps1`) are a public contract. See [`site/README.md`](site/README.md).

```bash
cd site
pnpm install --frozen-lockfile
pnpm dev        # or: pnpm build && pnpm preview
```

## Licensing

By contributing you agree that your work is licensed under `MIT OR Apache-2.0`, as stated in the README.

## Trademark

Do not use the Godot logo in this project. The word "Godot" is used only to describe what the tool works with, per the Godot Foundation's trademark policy.
