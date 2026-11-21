//! The Notes Context: Notes as Markdown files, each named by its Item ID
//! (ADR-0030), flat in one folder (ADR-0035).
//!
//! Owns every Notes rule; clients only render and forward input. The crate is
//! plain Rust with no FFI concerns; foreign-language clients go through
//! `pkm-bindings`.

mod store;
pub mod sync;

pub use pkm_base::files::{DiskFiles, Files, MemoryFiles};
pub use pkm_search::Snippet;
pub use store::{Found, Note, Notes, Row};
