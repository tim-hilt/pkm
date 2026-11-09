# Own JSON implementation instead of serde

The core reads and writes `.excalidraw` JSON with its own parser and serializer rather than `serde`/`serde_json`. This keeps the core's dependency surface to `uniffi` alone, and a purpose-built JSON value model makes it straightforward to round-trip unknown Element types and properties verbatim (ADR-0003).
