//! The Books Context: the PDFs and EPUBs a user has imported, their Markup and
//! Reading Progress.
//!
//! Owns everything that must behave identically on every platform. It never
//! renders or parses PDFs; clients do that with the platform's native PDF
//! engine (see `docs/adr/0001-native-pdf-rendering.md`). It reads EPUBs, but
//! doesn't draw them (`docs/adr/0005-core-parses-epubs.md`).
//!
//! The crate is plain Rust with no FFI concerns; foreign-language clients go
//! through `pkm-bindings`.

pub(crate) mod book_cache;
pub mod book_text;
pub mod contents;
pub mod details;
pub mod epub;
pub(crate) mod files;
pub(crate) mod json;
pub mod jump;
pub mod library;
pub mod listing;
pub mod markup;
pub(crate) mod percent;
pub mod progress;
pub mod search;
pub(crate) mod state;
pub(crate) mod store;
pub mod sync;
