//! iCloud Drive as a Cloud Provider: the user grants its root once, and the
//! Drawings are Synced with the `ExcalidrawNative` folder in it through the
//! directory storage (ADR-0012).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::storage::ConnectError;

/// The folder iCloud Drive's root is on every Apple platform.
const ROOT_NAME: &str = "com~apple~CloudDocs";

/// The folder the Drawings go into, in the root.
const FOLDER: &str = "ExcalidrawNative";

/// Connects to iCloud Drive through the folder `granted` that the user picked:
/// only iCloud Drive's root is accepted. Creates `ExcalidrawNative` in it if
/// missing and returns its path, the directory to Sync with. Keeps nothing.
pub fn connect(granted: &Path) -> Result<PathBuf, ConnectError> {
    if granted.file_name().is_none_or(|name| name != ROOT_NAME) {
        return Err(ConnectError::WrongFolder);
    }
    let folder = granted.join(FOLDER);
    match fs::create_dir(&folder) {
        Ok(()) => Ok(folder),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists && folder.is_dir() => Ok(folder),
        // The root is gone: iCloud Drive was turned off.
        Err(e) if e.kind() == io::ErrorKind::NotFound => Err(ConnectError::ICloudDriveUnavailable),
        Err(_) => Err(ConnectError::Unreachable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::contract::Scratch;

    /// A stand-in for iCloud Drive's root.
    fn root(scratch: &Scratch) -> PathBuf {
        let root = scratch.0.join(ROOT_NAME);
        fs::create_dir(&root).unwrap();
        root
    }

    #[test]
    fn connecting_creates_the_folder_in_the_root() {
        let scratch = Scratch::new("icloud");
        let root = root(&scratch);

        let folder = connect(&root).unwrap();

        assert_eq!(folder, root.join("ExcalidrawNative"));
        assert!(folder.is_dir());
    }

    #[test]
    fn connecting_again_keeps_the_folder_and_what_is_in_it() {
        let scratch = Scratch::new("icloud");
        let root = root(&scratch);
        let folder = connect(&root).unwrap();
        fs::write(folder.join("Sketch.excalidraw"), b"{}").unwrap();

        assert_eq!(connect(&root).unwrap(), folder);

        assert!(folder.join("Sketch.excalidraw").exists());
    }

    #[test]
    fn any_other_folder_is_refused() {
        let scratch = Scratch::new("icloud");
        let other = scratch.0.join("Documents");
        fs::create_dir(&other).unwrap();

        assert_eq!(connect(&other), Err(ConnectError::WrongFolder));
        assert_eq!(
            connect(&root(&scratch).join("Sub")),
            Err(ConnectError::WrongFolder)
        );
        assert!(!other.join("ExcalidrawNative").exists());
    }

    #[test]
    fn a_root_that_is_not_there_means_icloud_drive_is_off() {
        let scratch = Scratch::new("icloud");

        let missing = scratch.0.join(ROOT_NAME);

        assert_eq!(connect(&missing), Err(ConnectError::ICloudDriveUnavailable));
    }
}
