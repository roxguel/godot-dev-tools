# Contributing

Thanks for your interest. The project is young: the core works, and the most useful contributions right now are bug reports, testing on macOS, on arm64 and on other Linux distributions, feedback on the documents in [`docs/`](docs/README.md) and answers to the items in [`docs/open-questions.md`](docs/open-questions.md).

## Workflow

- Open an issue before a large change so the approach can be agreed first.
- Fork the repository (or, for the maintainer, work on a branch) and open a pull request against `main`. `main` is protected: nothing is pushed to it directly, and a pull request can only be merged once the checks pass.
- Name the branch for the kind of change: `feat/...`, `fix/...`, `docs/...`, `chore/...`, `ci/...`.
- Commits follow conventional commits with a scope: `feat(versions): ...`, `fix(addons): ...`, `docs(adr): ...`. One pull request is one change that can be reviewed on its own.
- Decisions that shape the architecture are recorded as ADRs in `docs/adr/`; behavior changes update the matching spec.
- A change that users can notice (behavior, output, files, distribution) adds a line to the `[Unreleased]` section of [`CHANGELOG.md`](CHANGELOG.md), in the style of the ones there.
- The maintainer reviews and merges every pull request. A pull request from a fork runs CI only after the maintainer approves the run, and never receives secrets.
- Report a security problem privately, as [SECURITY.md](SECURITY.md) says, not in a pull request or an issue.

## Before sending a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

CI runs these as the checks `Lint`, `Test` (Ubuntu, Windows and macOS), `Installers` (ShellCheck and a parse check of the install scripts) and `Dependencies` (`cargo deny`: advisories, licenses, sources), plus the site build when the site or the documents change. Code must compile and pass tests on Windows, Linux and macOS; CI checks all three. Keep OS-specific code in a platform module, and keep filesystem and network access behind traits so logic can be tested without them. A name or path that comes from outside (the command line, `gdt.toml`, `gdt.lock`, an archive) must go through `crates/godot-dev-tools/src/names.rs` before it is used to build a path, and a change that touches such code needs a regression test with a hostile input.

## Releases

Only the maintainer releases. The branch model, the steps, the checks after a release and what to do when a job fails are in [docs/releasing.md](docs/releasing.md); the settings behind them (branch and tag rules, the `release` environment, crates.io and the package managers) are in [docs/maintainers.md](docs/maintainers.md).

## The documentation site

`site/` is an [Astro Starlight](https://starlight.astro.build) site published at <https://gdt.roxguel.dev>. It reuses `docs/`, `CHANGELOG.md`, `SECURITY.md` and this file instead of copying them (`site/scripts/sync-docs.mjs` generates git-ignored pages from them), so edit those, not generated pages. The installers live in `site/public/install.sh` and `install.ps1`; their URLs (`/install.sh`, `/install.ps1`) are a public contract. See [`site/README.md`](site/README.md).

```bash
cd site
pnpm install --frozen-lockfile
pnpm dev        # or: pnpm build && pnpm preview
```

## Licensing

By contributing you agree that your work is licensed under `MIT OR Apache-2.0`, as stated in the README.

A new dependency must use a license listed in `deny.toml` (`cargo deny check` in CI) and in `about.toml`. The release workflow runs [cargo-about](https://github.com/EmbarkStudios/cargo-about) and puts `THIRD-PARTY-NOTICES.txt`, with the license of every library in the binary, in each archive. To see it:

```bash
cargo install cargo-about --version 0.9.2 --locked --features cli
cargo about generate --locked --fail -m crates/gdt/Cargo.toml -c about.toml -o THIRD-PARTY-NOTICES.txt about.hbs
```

## Trademark

Do not use the Godot logo in this project. The word "Godot" is used only to describe what the tool works with, per the Godot Foundation's trademark policy.
