# Packaging manifests

Manifests for the package managers, written for **0.1.0**. They are not used by the build: each one is copied to the place that serves it (below). The hashes are the SHA-256 of the release archives, from the `SHA256SUMS` of the release.

| Folder | Package manager | Goes to |
|---|---|---|
| `winget/` | Windows Package Manager | a pull request to [`microsoft/winget-pkgs`](https://github.com/microsoft/winget-pkgs), under `manifests/r/Roxguel/Gdt/0.1.0/` |
| `scoop/gdt.json` | Scoop | `bucket/gdt.json` of a bucket repository (`roxguel/scoop-bucket`) |
| `homebrew/gdt.rb` | Homebrew | `Formula/gdt.rb` of a tap repository (`roxguel/homebrew-tap`) |

## Checked

- `winget validate --manifest packaging/winget` passes (schema only; the install itself needs a local-manifest setting that is not changed here).
- The two Windows archives were downloaded and their SHA-256 match; both hold `gdt-v0.1.0-<target>/gdt.exe`, the path the winget manifest points to.
- `scoop/gdt.json` parses as JSON.
- Not run: `scoop install` and `brew install` / `brew audit --strict --online` / `brew test` (no Scoop or Homebrew here). Run them from the bucket and the tap before announcing.

## Next versions

- **winget:** `wingetcreate update Roxguel.Gdt --version <v> --urls <x64 zip> <arm64 zip> --submit`. Check the result: it has had bugs with several architectures.
- **Scoop:** `checkver` and `autoupdate` are in the manifest; the bucket's excavator, or `checkver.ps1 -Update`, bumps the version and hashes.
- **Homebrew:** `livecheck` is in the formula; `brew bump-formula-pr` (or editing the version and the four hashes by hand) updates it.
