# ADR 0002: Dual license MIT OR Apache-2.0

Status: Accepted (2026-10-29)

## Context

The project is open source from day one, like Godot (MIT). The Rust ecosystem convention is `MIT OR Apache-2.0`.

## Decision

License all code under either MIT or Apache-2.0, at the user's option. Ship `LICENSE-MIT` and `LICENSE-APACHE`. Contributions are accepted under the same terms (stated in the README).

## Consequences

- Maximum compatibility with other projects; Apache-2.0 adds an explicit patent grant.
- Third-party dependencies must be license-compatible; a `cargo deny` check will be added to CI.
- The Godot logo is not used, and the Godot trademark is referenced only per the Godot Foundation's trademark policy (see the README).
