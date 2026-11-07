//! Cloud Providers (ADR-0012): the storage interface, the directory storage,
//! the WebDAV storage, iCloud Drive, Google Drive and OneDrive with the OAuth
//! sign-in and the HTTP client they share, and the Sync engine and loop on
//! top of them. Blocking, no async runtime. What needs the operating system or
//! a blocking network sits behind the `native` feature (on by default); without
//! it the crate builds for `wasm32-unknown-unknown` (ADR-0016).

#[cfg(feature = "native")]
pub mod gdrive;
#[cfg(feature = "native")]
mod http;
#[cfg(feature = "native")]
pub mod icloud;
#[cfg(feature = "native")]
mod oauth;
#[cfg(feature = "native")]
pub mod onedrive;
#[cfg(feature = "native")]
mod percent;
pub mod schedule;
#[cfg(feature = "native")]
mod sha256;
pub mod step;
pub mod storage;
#[cfg(feature = "native")]
mod sync;
#[cfg(feature = "native")]
mod sync_loop;
#[cfg(feature = "native")]
pub mod webdav;

#[cfg(all(test, feature = "native"))]
mod fake_google;
#[cfg(all(test, feature = "native"))]
mod fake_http;
#[cfg(all(test, feature = "native"))]
mod fake_onedrive;

#[cfg(feature = "native")]
pub use gdrive::GoogleDriveStorage;
#[cfg(feature = "native")]
pub use onedrive::OneDriveStorage;
pub use schedule::{Schedule, Status, Timing};
pub use step::{Next, Progress, Request, Response, StepOutcome, SyncStep};
#[cfg(feature = "native")]
pub use storage::DirStorage;
pub use storage::{Blob, ConnectError, Storage};
#[cfg(feature = "native")]
pub use sync::{SyncState, sync_step, sync_step_reporting};
#[cfg(feature = "native")]
pub use sync_loop::{Observer, SyncLoop};
#[cfg(feature = "native")]
pub use webdav::WebDavStorage;
