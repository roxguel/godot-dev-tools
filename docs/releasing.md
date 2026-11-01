# Releasing

Status: Draft. Updated 2026-11-01. The steps up to the GitHub release were run for 0.1.0; the `crates` and `packaging` jobs have not run on a real release yet, so this document is corrected after the first one that uses them (0.1.1).

How a version of gdt is released. Only the maintainer does it: merging pull requests, creating a tag and approving the release are not open to contributors (see [maintainers.md](maintainers.md) for the settings that enforce it).

## How it works

- There is one long-lived branch, `main`. Every change reaches it through a pull request; there is no `develop` branch and no release branch.
- A version exists when a tag `vX.Y.Z` is pushed. Nothing in `main` is released until then, so `main` may hold work that is not in any release yet.
- The tag must equal `v` plus the version in `Cargo.toml`, and the tagged commit must already be in `main`; the workflow refuses anything else.
- Versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html). Until 1.0, a minor version can change the command line or the project files, and the changelog lists it. Only the latest release is supported.
- A tag with a hyphen (`v0.2.0-rc.1`) is a pre-release: it gets a GitHub pre-release and nothing else. It is not published to crates.io and does not open pull requests in the tap or the bucket, because those cannot be taken back cleanly.

What a stable tag produces, in order, each step after the first waiting for the maintainer's approval of the `release` environment:

| Job | Result |
|---|---|
| `notices`, `build` (six targets) | `THIRD-PARTY-NOTICES.txt` and the six archives, built with a fixed compiler. No approval needed |
| `publish` | `SHA256SUMS`, build provenance attestations and the GitHub release |
| `crates` | `godot-dev-tools` and `gdt` on crates.io (trusted publishing, no stored token) |
| `packaging` | A pull request in `roxguel/homebrew-tap` and one in `roxguel/scoop-bucket` |

winget is not part of the workflow; it is submitted by hand (below).

## Before

- `main` is green (CI) and the weekly advisories check has no open failure.
- The `[Unreleased]` section of `CHANGELOG.md` describes what changed for users. Every pull request that changes behavior, output, files or distribution adds its line when it is merged, so this is a review, not a rewrite.
- Specs in `docs/` and the roadmap match what is shipped.
- Nothing in the release workflow, the installers or the packaging changed since the last release without being tried: a change there is first tested in its own pull request (the release workflow runs, without publishing, on a pull request that touches it).

## Steps

1. **Release pull request** from a branch `release/X.Y.Z`, titled `chore(release): X.Y.Z`:
   - `version` in the root `Cargo.toml` and the `godot-dev-tools` dependency version next to it; run `cargo check --workspace` so `Cargo.lock` follows.
   - `CHANGELOG.md`: rename `[Unreleased]` to `[X.Y.Z] - YYYY-MM-DD`, add a new empty `[Unreleased]` above it, and update the compare links at the bottom.
   - Roadmap or README text that names the version, if any.
   - Locally: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`, `cargo test --workspace --locked` and `cargo publish --workspace --dry-run --locked`.
2. **Merge it.** For a risky release, release `X.Y.Z-rc.1` first the same way and try it (installers with `--version`, `--verify-attestation`); delete the pre-release afterwards.
3. **Tag the merge commit** on `main`:

   ```bash
   git fetch origin
   git tag -a vX.Y.Z origin/main -m "vX.Y.Z"
   git push origin vX.Y.Z
   ```

   Only a repository admin can create a tag `v*` (a ruleset), and the `release` environment only accepts tags `v*`.
4. **Approve** in the run of the Release workflow (Actions → the run → Review deployments). Approve `publish`; when it finishes, `crates` and `packaging` ask for the same approval and can be approved together.
5. **Check** (next section).
6. **Merge the pull requests** the `packaging` job opened in the tap and the bucket.
7. **winget:** render and submit the manifests.

   ```bash
   gh release download vX.Y.Z --pattern SHA256SUMS
   node packaging/render.mjs X.Y.Z SHA256SUMS out
   wingetcreate submit out/winget
   ```

   Then follow the validation on the pull request in `microsoft/winget-pkgs`.

## Check

```bash
gh release view vX.Y.Z                      # six archives and SHA256SUMS; "Pre-release" only for a hyphen
gh attestation verify gdt-vX.Y.Z-<target>.tar.gz --repo roxguel/godot-dev-tools \
  --signer-workflow roxguel/godot-dev-tools/.github/workflows/release.yml \
  --source-ref refs/tags/vX.Y.Z --deny-self-hosted-runners
curl -fsSL https://gdt.roxguel.dev/install.sh | sh -s -- --version X.Y.Z --dir /tmp/gdt --verify-attestation
cargo install gdt --version X.Y.Z --locked --root /tmp/gdt-crates
```

- The archives hold `gdt`, both license files, `THIRD-PARTY-NOTICES.txt` and the README.
- On Windows, `install.ps1 -Version X.Y.Z -VerifyAttestation`.
- `https://crates.io/crates/gdt` and `https://crates.io/crates/godot-dev-tools` show the version.
- The pull requests in the tap and the bucket change only the version and the hashes. Try them: `brew install roxguel/tap/gdt`, `scoop install roxguel/gdt`.

## When something fails

| Failure | What to do |
|---|---|
| A build or the checks fail before `publish` | Nothing was published. Fix it through a pull request, then delete the tag and tag again, or release the next patch (the admin bypass of the ruleset is needed to delete a tag) |
| `publish` is rejected ("not on main", "does not match") | The tag is on the wrong commit or the version differs. Delete the tag, fix, tag again |
| `crates` fails before uploading | Fix and re-run that job alone |
| `crates` published `godot-dev-tools` but not `gdt` | `cargo publish -p gdt --locked` from the tag, with a token or by re-running the job |
| A version on crates.io is wrong | It cannot be removed. `cargo yank --version X.Y.Z <crate>` and release the next patch |
| `packaging` warns that `PACKAGING_TOKEN` is not set, or fails to clone | The secret is missing or expired: create a new token and replace the secret ([maintainers.md](maintainers.md)), re-run the job; or run `packaging/render.mjs` and open the two pull requests by hand |
| The GitHub release is wrong after publishing | Do not move the tag. Release a patch; delete a pre-release only when it is a pre-release |

## Older lines

Fixes go to `main` and ship as the next patch. Only if an older version must be patched while `main` already holds incompatible work, create a branch `release/0.x` from the tag, take the fix there by pull request, and tag from it; the release workflow's check that the tagged commit is in `main` and the branch rules must then be extended to `release/*` first. Until 1.0 this is not expected.
