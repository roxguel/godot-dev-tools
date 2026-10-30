# Contributing

Thanks for your interest. The project is in its early design phase, so the most useful contributions right now are feedback on the documents in [`docs/`](docs/README.md) and answers to the items in [`docs/open-questions.md`](docs/open-questions.md).

## Workflow

- Open an issue before a large change so the approach can be agreed first.
- Work on a branch and open a pull request against `main`.
- Commits follow conventional commits with a scope: `feat(versions): ...`, `fix(addons): ...`, `docs(adr): ...`.
- Decisions that shape the architecture are recorded as ADRs in `docs/adr/`; behavior changes update the matching spec.

## Before sending a pull request

```bash
cargo fmt --all
cargo clippy --workspace --all-targets
cargo test --workspace
```

Code must compile and pass tests on Windows, Linux and macOS; CI checks all three. Keep OS-specific code in a platform module, and keep filesystem and network access behind traits so logic can be tested without them.

## Licensing

By contributing you agree that your work is licensed under `MIT OR Apache-2.0`, as stated in the README.

## Trademark

Do not use the Godot logo in this project. The word "Godot" is used only to describe what the tool works with, per the Godot Foundation's trademark policy.
