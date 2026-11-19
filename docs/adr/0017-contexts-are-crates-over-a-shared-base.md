# Contexts are crates over a shared base, with one bindings library

Amends ADR-0001 (one Rust core) and ADR-0009 (no dependencies).

Each Context's logic is its own crate (`pkm-drawing`, `pkm-books`). What they and `pkm-cloud` share lives once in `pkm-base`: the JSON reader and writer, base64, SHA-256, the clock and the random generator. `pkm-base` has no dependencies, and `pkm-drawing` depends only on it, so ADR-0009 holds for both (the Makefile checks it). `pkm-bindings` stays the only UniFFI library and XCFramework, since two Rust static libraries can't be linked into one app.

## Considered Options

- **One crate for everything**: Contexts would depend on each other's internals.
- **A bindings library per Context**: duplicate runtime symbols when linked into one app.
- **Each crate keeps its own copies**: what the cloud crate did with SHA-256 and the `base64` crate.

## Consequences

- `pkm-cloud` no longer depends on the `base64` crate; the wasm build shrank by 184 bytes.
- `pkm-notes` also depends on `unicode-normalization` to fold accents in Search; `pkm-books` links it already, so the app doesn't grow.
