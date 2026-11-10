//! Writing files so a crash never leaves a torn one.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};

/// Makes the bytes written to `file` reach the disk before any later write
/// does, e.g. the rename that puts it in place: a crash then leaves the old
/// file or the new one, never a torn one. Doesn't wait for the disk itself,
/// so the latest writes may be lost, which is far cheaper on Apple's
/// platforms (`F_BARRIERFSYNC` rather than `F_FULLFSYNC`).
pub(crate) fn flush_ordered(file: &File) -> io::Result<()> {
    #[cfg(target_vendor = "apple")]
    {
        use std::os::fd::AsRawFd;
        // SAFETY: `fcntl` with a command that takes no argument, on a
        // descriptor `file` keeps open.
        if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_BARRIERFSYNC) } == -1 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
    #[cfg(not(target_vendor = "apple"))]
    file.sync_data()
}

/// A name no other call in this process, or an earlier one, has used for a
/// scratch file or folder. Starts with the process ID.
pub(crate) fn unique_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Relaxed))
}

/// `result`, except that a missing file or folder (`NotFound`) is `Ok`: there
/// was nothing to do.
pub(crate) fn ignore_not_found(result: io::Result<()>) -> io::Result<()> {
    match result {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Writes `bytes` to a new file in the directory `staging`, then renames it to
/// `path`, so `path` is the old file or the new one, never a torn one. A
/// failure leaves no staged file.
pub(crate) fn write_atomically(staging: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = staging.join(unique_name());
    let written = (|| {
        let mut file = File::create_new(&staged)?;
        file.write_all(bytes)?;
        flush_ordered(&file)?;
        fs::rename(&staged, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&staged);
    }
    written
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_names_differ() {
        assert_ne!(unique_name(), unique_name());
    }

    #[test]
    fn an_atomic_write_replaces_the_file_and_leaves_nothing_staged() {
        let dir = std::env::temp_dir().join(format!("pkm-books-files-{}", unique_name()));
        let staging = dir.join("staging");
        fs::create_dir_all(&staging).unwrap();
        let path = dir.join("file");
        write_atomically(&staging, &path, b"one").unwrap();
        write_atomically(&staging, &path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
        // A missing folder fails, and cleans up.
        assert!(write_atomically(&staging, &dir.join("no/file"), b"x").is_err());
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
        ignore_not_found(fs::remove_dir_all(&dir)).unwrap();
    }

    #[test]
    fn a_missing_file_is_ignored_and_other_errors_are_not() {
        let missing = Err(io::Error::from(io::ErrorKind::NotFound));
        assert!(ignore_not_found(missing).is_ok());
        let denied = Err(io::Error::from(io::ErrorKind::PermissionDenied));
        assert!(ignore_not_found(denied).is_err());
    }
}
