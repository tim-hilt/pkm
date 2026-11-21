//! The Bookmarks Context: Bookmarks as Markdown files, each named by its Item
//! ID (Bookmarks ADR-0002).
//!
//! Owns every Bookmarks rule; clients only download pages and show the result.
//! The crate is plain Rust with no FFI concerns; foreign-language clients go
//! through `pkm-bindings`.

mod address;
mod article;
mod extract;
mod html;
mod images;
mod store;

pub use address::{is_document, normalise};
pub use article::{Article, read_article};
pub use extract::{Details, extract};
pub use pkm_base::files::{DiskFiles, Files, MemoryFiles};
pub use store::{ArticleProgress, Bookmark, Bookmarks, Saved};
