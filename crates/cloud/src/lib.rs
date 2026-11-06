//! Cloud Providers (ADR-0012): the storage interface, the directory storage,
//! the WebDAV storage and the HTTP client they share, and the Sync engine and
//! loop on top of them. Blocking, no async
//! runtime.

mod http;
mod percent;
pub mod storage;
mod sync;
mod sync_loop;
pub mod webdav;

pub use storage::{Blob, ConnectError, DirStorage, Storage};
pub use sync::{Progress, StepOutcome, SyncState, sync_step, sync_step_reporting};
pub use sync_loop::{Observer, Status, SyncLoop, Timing};
pub use webdav::WebDavStorage;
