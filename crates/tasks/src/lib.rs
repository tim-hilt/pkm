//! The Tasks Context: Tasks as Markdown files, each named by its
//! Item ID (ADR-0021).
//!
//! Owns every Tasks rule; clients only render and forward input. The crate is
//! plain Rust with no FFI concerns; foreign-language clients go through
//! `pkm-bindings`.

mod files;
mod query;
mod quick_add;
mod store;
pub mod sync;

pub use files::{DiskFiles, Files, Listed, MemoryFiles, Stamp};
pub use query::{Query, QueryError, typed as typed_query, unfinished_tag};
pub use quick_add::{QuickAdd, QuickShortcut, ShortcutMeaning, TagQuery};
pub use store::{Blocker, Closing, Error, Filter, Row, Task, Tasks};
