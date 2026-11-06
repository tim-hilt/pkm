//! Cloud Providers (ADR-0012): the storage interface, the directory storage,
//! the WebDAV storage and the HTTP client they share. Blocking, no async
//! runtime.

mod http;
mod percent;
pub mod storage;
pub mod webdav;

pub use storage::{Blob, ConnectError, DirStorage, Storage};
pub use webdav::WebDavStorage;
