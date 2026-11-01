# Packaging manifests

`render.mjs` writes the manifests for one release from its `SHA256SUMS`; the files in this folder are its output for **0.1.0**. They are not used by the build.

```bash
node packaging/render.mjs 0.1.0 SHA256SUMS out 2026-11-01
```

| Folder | Package manager | Served from |
|---|---|---|
| `winget/` | Windows Package Manager | a pull request to [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs), under `manifests/r/Roxguel/Gdt/<version>/` (by hand: `wingetcreate submit`) |
| `scoop/gdt.json` | Scoop | `bucket/gdt.json` of [`roxguel/scoop-bucket`](https://github.com/roxguel/scoop-bucket) |
| `homebrew/gdt.rb` | Homebrew | `Formula/gdt.rb` of [`roxguel/homebrew-tap`](https://github.com/roxguel/homebrew-tap) |

## Each release

For a stable tag, the `packaging` job of the release workflow (after the GitHub release exists, and with the approval of the `release` environment) downloads the release's `SHA256SUMS`, runs `render.mjs`, and opens a pull request in the tap and another in the bucket with the new formula and manifest. The maintainer merges them. It needs a secret, `PACKAGING_TOKEN`, in the `release` environment: a fine-grained personal access token limited to those two repositories, with **Contents** and **Pull requests** set to read and write. Without the secret the job warns and does nothing.

winget is not automated: submit it by hand from the rendered `winget/` folder (`wingetcreate submit <folder>`) and check the pull request's validation.

## Checked

- `winget validate` passes on the output (schema only; installing from a local manifest needs a setting that is not changed here).
- For 0.1.0 the two Windows archives were downloaded: their SHA-256 match, and both hold `gdt-v0.1.0-<target>/gdt.exe`, the path the winget manifest points to.
- `render.mjs` refuses a pre-release version and a `SHA256SUMS` that lacks an archive.
- Not run: `scoop install` and `brew install` / `brew audit` (no Scoop or Homebrew on the machine that wrote them).
