# ADR 0008: Credentials come from the environment or the OS credential store

Status: Accepted (2026-10-30)

## Context

The Asset Store uses bearer API keys that the user creates on the website. CI needs them as secrets; developers need them stored safely. Other tools differ: `npm` writes `~/.npmrc` in plain text, `cargo login` writes a `0600` file, `gh` uses the system keychain with a file fallback.

## Decision

Order of lookup: the environment variable (`GDT_STORE_TOKEN`), then the OS credential store (the `keyring` crate), then an owner-only file with a warning, only when no credential store exists. Tokens are accepted only through a hidden prompt or stdin, never as an argument; never printed or logged; sent only to the host they belong to; and never stored in project files. Details in [authentication.md](../specs/authentication.md).

## Consequences

- CI works with a plain secret and no state on disk.
- Developers get the system keychain by default.
- A token file on headless machines is the weakest case, which is why it warns.
