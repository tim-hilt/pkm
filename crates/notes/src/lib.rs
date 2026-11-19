//! The Notes Context: Notes as Markdown files, each named by its Item ID
//! (ADR-0030).
//!
//! Owns every Notes rule; clients only render and forward input. The crate is
//! plain Rust with no FFI concerns; foreign-language clients go through
//! `pkm-bindings`.

mod folder;
mod store;

pub use folder::{Counts, Folder, FolderError};
pub use pkm_base::files::{DiskFiles, Files, MemoryFiles};
pub use store::{Note, Notes, Row};
