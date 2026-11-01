## What and why

<!-- What changes, and why. Link the issue if there is one. -->

## Checked

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings` and `cargo test --workspace --locked` pass (or this only changes docs, CI or packaging)
- [ ] A change in behavior, output, files or distribution has its line in `[Unreleased]` of `CHANGELOG.md`
- [ ] The specs in `docs/` and the ADRs match the change
- [ ] Names and paths that come from outside go through `names.rs`; nothing new follows a symbolic link or writes outside its folder

<!-- Not tried by hand (a platform, a flow, something that only runs on a tag)? Say it here. -->
