---
title: Verify a download
description: Check the SHA-256 checksum and the build provenance of a gdt release by hand.
---

Every release publishes, next to the archives:

- a `SHA256SUMS` file with the SHA-256 of each archive, and
- a signed **build provenance attestation** for each archive, which says it was built from this repository by its
  release workflow.

The installers check the first for you. The second needs the [GitHub CLI](https://cli.github.com/), so it is opt-in
there (`--verify-attestation`). Here is how to do both by hand.

Archives are named `gdt-v<version>-<target>.tar.gz` (`.zip` on Windows), with `<target>` one of
`x86_64-pc-windows-msvc`, `aarch64-pc-windows-msvc`, `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`,
`x86_64-apple-darwin` or `aarch64-apple-darwin`.

## Checksum

Download the archive and `SHA256SUMS` from the [release page](https://github.com/roxguel/godot-dev-tools/releases) into
the same folder, then:

```sh
# Linux
sha256sum --check --ignore-missing SHA256SUMS

# macOS
shasum -a 256 --check --ignore-missing SHA256SUMS
```

```powershell
# Windows: compare the two values by eye
(Get-FileHash .\gdt-v0.1.0-x86_64-pc-windows-msvc.zip -Algorithm SHA256).Hash
Select-String 'x86_64-pc-windows-msvc' .\SHA256SUMS
```

You want `OK` for your archive (or two identical hashes). Anything else: do not run it, and [report it](/security/).

## Build provenance

```sh
gh attestation verify gdt-v0.1.0-x86_64-unknown-linux-musl.tar.gz --repo roxguel/godot-dev-tools
```

This asks GitHub for the attestation of that exact file and checks its signature. A pass means the archive is the
output of this repository's release workflow; it does not replace reading the source. `gh` needs to be signed in
(`gh auth login`).

A checksum fetched from the same place as the archive proves the download was not corrupted; the attestation is what
ties the archive to the build.

## The installers themselves

`install.sh` and `install.ps1` are served from this site. [Read them](/install/) before running them; the page shows
their SHA-256 so you can compare it with what you download:

```sh
curl -fsSL https://gdt.roxguel.dev/install.sh | sha256sum
```
