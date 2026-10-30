# Authentication

Status: Draft. Based on the Asset Store's public OpenAPI document (<https://store.godotengine.org/api/v1/openapi.json>, API version 1.1.0) read on 2026-10-30. Items marked *unverified* need an account to confirm.

## What the Asset Store API offers

- One security scheme: an HTTP **bearer API key** (`Authorization: Bearer <token>`).
- `GET /api/v1/auth/introspection` returns whether the request is authenticated, the method, the user id and name. Use it to validate a token.
- `POST /api/v1/auth/token` creates a new API key with a `description` and a `lifespan` in seconds, so keys can expire. It requires being authenticated already, so the **first** key must come from the website. `DELETE /api/v1/auth/token` revokes a key.
- There is no OAuth or device-code flow in the document.
- Reading assets and their releases (`GET /api/v1/assets/...`, `GET /api/v1/releases/{publisher}/{asset}/`) needs no token. A release has `version`, `stable`, `size`, `min_godot_version`, `max_godot_version` and a `download_url`.
- For a free asset the `download_url` is a presigned storage URL valid for about 10 minutes, and a plain GET works without a token (checked on a free asset).
- A release carries **no checksum**. Assets also expose `price_cent` and `license_type`.
- *Unverified:* whether a purchased asset's `download_url` is only returned when the bearer token is sent, and how a paid asset behaves without one.
- *Unverified:* the page where a user creates an API key.

## Commands

```bash
gdt auth login store                      # opens the token page, then asks for the token (input hidden)
gdt auth login store --with-token < file  # read the token from stdin (scripts)
gdt auth status                           # which services are signed in, as which user
gdt auth logout store [--revoke]          # forget the token; with --revoke also delete it on the server
```

`login` checks the token with `introspection` before saving it. The token is **never accepted as a command-line argument** (it would end up in shell history and in the process list); only through the hidden prompt or stdin.

The group is `auth`, not `addon store`, because other services will need credentials too (for example a GitHub token). This follows `gh auth` and `cargo login`.

## Where the token lives

First match wins when `gdt` needs it:

1. The environment variable `GDT_STORE_TOKEN`. This is for CI: set it as the platform's secret. It is never written to disk.
2. The operating system's credential store, through the `keyring` crate: Windows Credential Manager, macOS Keychain, Secret Service (libsecret) on Linux.
3. A file `credentials.toml` in the config directory with owner-only permissions (`0600`), used only when no credential store is available (for example a headless Linux box). `gdt` prints a warning when it does this.

Rules:

- The token is sent **only to `store.godotengine.org`**. It is never sent to the storage host of the presigned download URL (that would also invalidate the signature), nor to any other host.
- It is never printed, never logged (also not with `--verbose`) and never written to `gdt.toml`, `gdt.lock` or the project folder.
- Prefer tokens with an expiry (`lifespan`) and a description such as `gdt on <machine>`.

## Using it in CI

CI platforms hand secrets to a job as environment variables, and `GDT_STORE_TOKEN` is the first place `gdt` looks, so a pipeline never needs `gdt auth login`:

```yaml
# GitHub Actions
- run: gdt sync
  env:
    GDT_STORE_TOKEN: ${{ secrets.GDT_STORE_TOKEN }}
    GH_TOKEN: ${{ secrets.ADDONS_READ_TOKEN }}   # only if an addon comes from a private repository
```

`gdt auth login store --with-token` is for local scripts and for machines where a credential store exists. A CI runner has none, so using it there would only fall back to a file for no benefit.

Good practice for the token: a dedicated key with an expiry, stored as an encrypted secret, never echoed. Secrets are not passed to workflows triggered by pull requests from forks, so those runs install free assets only.

## Private GitHub repositories

A `github:` source in a private repository needs a token with read access to it. `gdt` reads `GH_TOKEN` or `GITHUB_TOKEN` and sends it only to `api.github.com`; the repository is fetched as an archive through the API, so no `git` credentials are involved.

- The `GITHUB_TOKEN` that GitHub Actions provides only reaches the repository running the workflow, **not other private repositories**. For those, create:
  - a **fine-grained personal access token** limited to the repositories needed, with the single permission *Contents: read*, and store it as a secret; or, better for organizations,
  - a **GitHub App** installation token (short-lived), minted in the workflow.
- Public repositories need no token; one only raises the rate limit.
- Other hosts (GitLab, a private server) are reached with `git`, using whatever credentials the machine already has (SSH key, credential helper, deploy key).

## Behavior

- Free assets never need a token; `gdt addon add` works signed out.
- If an asset needs a purchase or a token the user does not have, the error says so, shows the asset's `store_url`, and exits with code 6. `gdt` never buys anything.
- With a token, it is sent when requesting releases of a paid asset (*unverified* that this is what the API expects; to be confirmed with an account before implementing).
