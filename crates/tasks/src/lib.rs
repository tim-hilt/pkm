//! The Tasks Context: Tasks planned as Active or in the Backlog, each a
//! Markdown file named by its Item ID (ADR-0021).
//!
//! Owns every Tasks rule; clients only render and forward input. The crate is
//! plain Rust with no FFI concerns; foreign-language clients go through
//! `pkm-bindings`.

mod cache;
mod files;
mod history;
mod quick_add;
mod store;
pub mod sync;

pub use files::{DiskFiles, Files, Listed, MemoryFiles, Stamp};
pub use quick_add::{QuickAdd, QuickShortcut, ShortcutMeaning, TagQuery};
pub use store::{Blocker, Closing, Error, Planning, Row, Task, Tasks};
