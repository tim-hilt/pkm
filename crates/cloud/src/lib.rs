//! Cloud Providers (ADR-0012): the storage interface, the directory storage,
//! the WebDAV storage, iCloud Drive, Google Drive and OneDrive with the OAuth
//! sign-in and the HTTP client they share, and the Sync engine and loop on
//! top of them. Blocking, no async runtime.

pub mod gdrive;
mod http;
pub mod icloud;
mod oauth;
pub mod onedrive;
mod percent;
mod sha256;
pub mod storage;
mod sync;
mod sync_loop;
pub mod webdav;

#[cfg(test)]
mod fake_google;
#[cfg(test)]
mod fake_http;
#[cfg(test)]
mod fake_onedrive;

pub use gdrive::GoogleDriveStorage;
pub use onedrive::OneDriveStorage;
pub use storage::{Blob, ConnectError, DirStorage, Storage};
pub use sync::{Progress, StepOutcome, SyncState, sync_step, sync_step_reporting};
pub use sync_loop::{Observer, Status, SyncLoop, Timing};
pub use webdav::WebDavStorage;
