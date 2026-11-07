# The web app binds the core with hand-written wasm-bindgen

The web app reaches the core through a new crate, `crates/web`, that wraps it with hand-written `wasm-bindgen` exports mirroring the UniFFI surface of `crates/bindings` (same names and shapes where the type system allows), so that the TS client can follow the Swift client closely. Element geometry bypasses the bindings as typed-array views onto wasm memory, as in ADR-0007; the TextMeasurer port becomes an imported JS function. `crates/web` depends on the core only, not on `crates/cloud`.

## Considered Options

- **uniffi-bindgen-react-native (ubrn) on `crates/bindings`**: generated from the existing annotations, so the API would match Swift's exactly, but adds a generator, its runtime package and RustBuffer serialisation to every call; rejected to keep dependencies and bundle size low.
- **Raw `extern "C"` exports with hand-written TS glue**: a few KB smaller, but every string and list would be marshalled by hand.

## Consequences

- Every change to the UniFFI surface that the web app needs is made twice, in `crates/bindings` and `crates/web`.
