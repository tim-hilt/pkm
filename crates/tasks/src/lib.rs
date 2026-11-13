//! The Tasks Context: Tasks planned as Active or in the Backlog, each a
//! Markdown file named by its Item ID (ADR-0021).
//!
//! Owns every Tasks rule; clients only render and forward input. The crate is
//! plain Rust with no FFI concerns; foreign-language clients go through
//! `pkm-bindings`.

mod history;
mod store;

pub use store::{Closing, Error, Planning, Task, Tasks};
