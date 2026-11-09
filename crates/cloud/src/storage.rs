//! The storage interface every Cloud Provider implements (ADR-0012), and the
//! directory storage: the stand-in cloud of tests, later iCloud Drive.
//!
//! A storage is plain remote file storage: named blobs in one folder, no
//! transactions, no server logic. Everything above it is in the Sync engine.

use std::fmt;
#[cfg(feature = "native")]
use std::fs::{self, File};
use std::io::{self, Read};
#[cfg(feature = "native")]
use std::path::{Path, PathBuf};
#[cfg(feature = "native")]
use std::sync::Arc;
#[cfg(feature = "native")]
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
#[cfg(feature = "native")]
use std::time::UNIX_EPOCH;

/// A blob in a storage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    pub name: String,
    /// Differs from what it was before once the blob is overwritten: the
    /// ETag, or modified time and size where the storage has none. Drawings,
    /// unlike Leaf's files, are overwritten, so a name alone says nothing.
    pub marker: String,
}

/// Why connecting to a Cloud Provider failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectError {
    /// WebDAV: the server rejected the username or password.
    WrongCredentials,
    /// No answer: wrong host, no network, an untrusted certificate or a
    /// server that is down.
    Unreachable,
    /// WebDAV: the URL doesn't start with `https://`. The password would
    /// travel in plain text.
    NotHttps,
    /// WebDAV: the server answers, but not as a WebDAV server.
    NotWebDav,
    /// iCloud Drive: it is off, or the Device is not signed in.
    ICloudDriveUnavailable,
    /// iCloud Drive: the folder granted is not iCloud Drive's root.
    WrongFolder,
    /// OAuth: the user refused consent on the sign-in page.
    AccessDenied,
    /// OAuth: the sign-in did not go through: the redirect is not the answer
    /// to this sign-in, or the provider refused the code.
    SignInFailed,
    /// OneDrive: a work or school account, which is not supported.
    UnsupportedAccount,
}

/// The variant's name; clients word the message for the user.
impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ConnectError {}

/// Named blobs in one folder.
///
/// Names are plain: non-empty, without `/`, not `.` or `..`, and not starting
/// with a dot. Anything else is `InvalidInput`.
///
/// Errors are `io::Error`s. Credentials the storage rejects are
/// `PermissionDenied`, and a storage without space left is `StorageFull`, so
/// both can be told from being offline.
pub trait Storage: Send + Sync {
    /// Stores everything `data` yields as the blob `name`, replacing any
    /// previous one. A blob is never visible under `name` before all of it is
    /// written, and a failed `put` (`data` erroring too) leaves no trace.
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()>;

    /// The content of blob `name`; none if there is no such blob.
    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>>;

    /// All blobs, in no order.
    fn list(&self) -> io::Result<Vec<Blob>>;

    /// Deletes blob `name`. Deleting a blob that isn't there does nothing.
    fn delete(&self, name: &str) -> io::Result<()>;
}

#[cfg(feature = "native")]
/// Checks that `name` is a plain blob name, see `Storage`.
pub(crate) fn check_name(name: &str) -> io::Result<()> {
    if !name.is_empty() && !name.contains('/') && !name.starts_with('.') {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name:?} is not a blob name"),
        ))
    }
}

#[cfg(feature = "native")]
/// A name no other call in this process, or an earlier one, has used for a
/// scratch file. Starts with the process ID.
pub(crate) fn unique_name() -> String {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    format!("{}-{}", std::process::id(), NEXT.fetch_add(1, Relaxed))
}

#[cfg(feature = "native")]
/// Makes the bytes written to `file` reach the disk before any later write
/// does, e.g. the rename that puts it in place: a crash then leaves the old
/// file or the new one, never a torn one. Doesn't wait for the disk itself,
/// which is far cheaper on Apple platforms (`F_BARRIERFSYNC`).
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

#[cfg(feature = "native")]
/// Writes `bytes` to `path` so that it is whole with either its old or its new
/// content whatever happens part-way: into a hidden sibling first, then renamed
/// over `path`. A failure leaves no sibling.
pub(crate) fn write_file_atomically(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let staged = path.with_file_name(format!(".{}.tmp", unique_name()));
    let written = (|| {
        let mut file = File::create_new(&staged)?;
        io::Write::write_all(&mut file, bytes)?;
        flush_ordered(&file)?;
        fs::rename(&staged, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&staged);
    }
    written
}

#[cfg(feature = "native")]
/// Where blobs are written before they are renamed into place. Starts with a
/// dot, so it is no valid name and never listed.
const INCOMING: &str = ".incoming";

#[cfg(feature = "native")]
/// Asks the system to download a file that is in iCloud Drive only.
type RequestDownload = Arc<dyn Fn(&Path) + Send + Sync>;

#[cfg(feature = "native")]
/// A storage in a directory of the file system.
///
/// In iCloud Drive a file that is not on the Device is a hidden placeholder,
/// `.name.icloud`. Such a blob is absent until it is downloaded: listing or
/// reading it asks the client to download it, and a later call finds it.
pub struct DirStorage {
    root: PathBuf,
    request_download: Option<RequestDownload>,
}

#[cfg(feature = "native")]
impl fmt::Debug for DirStorage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("DirStorage")
            .field("root", &self.root)
            .finish()
    }
}

#[cfg(feature = "native")]
impl DirStorage {
    /// The storage in directory `root`, created if missing. Leftovers of
    /// `put`s that never finished are deleted, except those of this process:
    /// they belong to another storage in the same directory that is writing.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        let incoming = root.join(INCOMING);
        fs::create_dir_all(&incoming)?;
        let own = format!("{}-", std::process::id());
        for entry in fs::read_dir(&incoming)? {
            let entry = entry?;
            if !entry.file_name().to_string_lossy().starts_with(&own) {
                match fs::remove_file(entry.path()) {
                    Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                    _ => {}
                }
            }
        }
        Ok(Self {
            root,
            request_download: None,
        })
    }

    /// Like `open`, for a directory in iCloud Drive: `request_download` is
    /// called with the path a not-yet-downloaded blob will have once the
    /// system has downloaded it.
    pub fn open_downloading(
        root: impl Into<PathBuf>,
        request_download: impl Fn(&Path) + Send + Sync + 'static,
    ) -> io::Result<Self> {
        let mut storage = Self::open(root)?;
        storage.request_download = Some(Arc::new(request_download));
        Ok(storage)
    }

    /// The placeholder `.<name>.icloud` of blob `name`.
    fn placeholder(&self, name: &str) -> PathBuf {
        self.root.join(format!(".{name}.icloud"))
    }

    /// The name of the blob a placeholder file stands for.
    fn placeholder_target(file_name: &str) -> Option<&str> {
        file_name
            .strip_prefix('.')?
            .strip_suffix(".icloud")
            .filter(|name| !name.is_empty())
    }

    fn download(&self, name: &str) {
        if let Some(request) = &self.request_download {
            request(&self.root.join(name));
        }
    }

    fn path(&self, name: &str) -> io::Result<PathBuf> {
        check_name(name)?;
        Ok(self.root.join(name))
    }

    /// Streams `data` into a new file in `INCOMING`, then renames it to
    /// `path`. A failure leaves no staged file.
    fn write_atomically(&self, path: &Path, data: &mut dyn Read) -> io::Result<()> {
        let staged = self.root.join(INCOMING).join(unique_name());
        let written = (|| {
            let mut file = File::create_new(&staged)?;
            io::copy(data, &mut file)?;
            flush_ordered(&file)?;
            fs::rename(&staged, path)
        })();
        if written.is_err() {
            let _ = fs::remove_file(&staged);
        }
        written
    }
}

#[cfg(feature = "native")]
impl Storage for DirStorage {
    fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
        let path = self.path(name)?;
        self.write_atomically(&path, data)
    }

    fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
        match File::open(self.path(name)?) {
            Ok(file) if file.metadata()?.is_file() => Ok(Some(Box::new(file))),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                if self.placeholder(name).exists() {
                    self.download(name);
                }
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    fn list(&self) -> io::Result<Vec<Blob>> {
        let mut blobs = Vec::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let meta = entry.metadata()?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(target) = Self::placeholder_target(&name) {
                self.download(target);
            } else if meta.is_file() && !name.starts_with('.') {
                let modified = meta
                    .modified()?
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |t| t.as_nanos());
                blobs.push(Blob {
                    name,
                    marker: format!("{modified}-{}", meta.len()),
                });
            }
        }
        Ok(blobs)
    }

    fn delete(&self, name: &str) -> io::Result<()> {
        for path in [self.path(name)?, self.placeholder(name)] {
            match fs::remove_file(path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}

/// The content of blob `name` in memory, for small blobs. None if missing.
#[cfg(all(test, feature = "native"))]
pub(crate) fn get_bytes(storage: &dyn Storage, name: &str) -> io::Result<Option<Vec<u8>>> {
    storage
        .get(name)?
        .map(|mut data| {
            let mut bytes = Vec::new();
            data.read_to_end(&mut bytes)?;
            Ok(bytes)
        })
        .transpose()
}

/// The contract every `Storage` keeps, as tests. A storage runs them with
/// `storage_contract_tests!(make)`, where `make()` returns a new, empty
/// `Storage` (and whatever must stay alive with it), or none to skip the
/// tests, e.g. when the server they need isn't installed.
#[cfg(all(test, feature = "native"))]
pub(crate) mod contract {
    use super::*;
    use std::time::Duration;

    /// A fresh directory under the system temp dir, removed on drop.
    pub struct Scratch(pub PathBuf);

    impl Scratch {
        pub fn new(prefix: &str) -> Self {
            static NEXT: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "pkm-{prefix}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Relaxed)
            ));
            fs::create_dir_all(&dir).unwrap();
            Scratch(dir)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    pub fn put_bytes(storage: &dyn Storage, name: &str, bytes: &[u8]) {
        storage.put(name, &mut &bytes[..]).unwrap();
    }

    pub fn names(storage: &dyn Storage) -> Vec<String> {
        let mut names: Vec<_> = storage
            .list()
            .unwrap()
            .into_iter()
            .map(|b| b.name)
            .collect();
        names.sort();
        names
    }

    fn marker(storage: &dyn Storage, name: &str) -> String {
        let blobs = storage.list().unwrap();
        blobs.into_iter().find(|b| b.name == name).unwrap().marker
    }

    /// Yields `bytes`, then fails; asks `during` after the first read.
    struct Interrupted<'a> {
        bytes: &'a [u8],
        during: Option<Box<dyn FnOnce() + 'a>>,
    }

    impl Read for Interrupted<'_> {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.bytes.is_empty() {
                return Err(io::Error::other("connection lost"));
            }
            let n = self.bytes.read(buf)?;
            if let Some(during) = self.during.take() {
                during();
            }
            Ok(n)
        }
    }

    pub fn a_blob_reads_back_as_stored(storage: &dyn Storage) {
        put_bytes(storage, "a blob.excalidraw", b"hello");
        let bytes = get_bytes(storage, "a blob.excalidraw").unwrap();
        assert_eq!(bytes.as_deref(), Some(&b"hello"[..]));
    }

    pub fn a_missing_blob_reads_as_none_and_deleting_it_does_nothing(storage: &dyn Storage) {
        assert!(get_bytes(storage, "nothing").unwrap().is_none());
        storage.delete("nothing").unwrap();
    }

    pub fn putting_again_replaces_the_blob(storage: &dyn Storage) {
        put_bytes(storage, "blob", b"first version");
        put_bytes(storage, "blob", b"second");
        assert_eq!(get_bytes(storage, "blob").unwrap().unwrap(), b"second");
        assert_eq!(storage.list().unwrap().len(), 1);
    }

    pub fn a_deleted_blob_is_gone(storage: &dyn Storage) {
        put_bytes(storage, "one", b"1");
        put_bytes(storage, "two", b"22");
        storage.delete("one").unwrap();
        assert!(get_bytes(storage, "one").unwrap().is_none());
        assert_eq!(names(storage), ["two"]);
    }

    pub fn listing_gives_the_names_of_all_blobs(storage: &dyn Storage) {
        assert!(names(storage).is_empty());
        put_bytes(storage, "Sketch 2.excalidraw", b"1");
        put_bytes(storage, "Ünï.excalidraw", b"22");
        assert_eq!(names(storage), ["Sketch 2.excalidraw", "Ünï.excalidraw"]);
    }

    pub fn the_marker_changes_when_the_blob_is_overwritten(storage: &dyn Storage) {
        put_bytes(storage, "blob", b"aaaa");
        put_bytes(storage, "other", b"aaaa");
        let (before, other) = (marker(storage, "blob"), marker(storage, "other"));
        // Coarse clocks (WebDAV's one second) must see the second write too.
        std::thread::sleep(Duration::from_millis(1100));

        // The same size, so only the time or the ETag can tell.
        put_bytes(storage, "blob", b"bbbb");

        assert_ne!(marker(storage, "blob"), before);
        assert_eq!(marker(storage, "other"), other, "untouched blobs keep it");
    }

    pub fn a_blob_is_not_visible_while_it_is_written(storage: &dyn Storage) {
        put_bytes(storage, "blob", b"old");
        let mut data = Interrupted {
            bytes: b"new content",
            during: Some(Box::new(|| {
                // Neither the new content nor a part of it, nor a stray name.
                let during = get_bytes(storage, "blob").unwrap().unwrap();
                assert_eq!(during, b"old");
                assert_eq!(names(storage), ["blob"]);
            })),
        };
        // Fails after the first read, so the interrupted case runs here too.
        assert!(storage.put("blob", &mut data).is_err());
        assert_eq!(get_bytes(storage, "blob").unwrap().unwrap(), b"old");
    }

    pub fn a_failed_put_leaves_nothing_behind(storage: &dyn Storage) {
        let mut data = Interrupted {
            bytes: b"partial",
            during: None,
        };
        assert!(storage.put("new", &mut data).is_err());
        assert!(get_bytes(storage, "new").unwrap().is_none());
        assert!(storage.list().unwrap().is_empty());
    }

    macro_rules! storage_contract_tests {
        ($make:expr) => {
            $crate::storage::contract::storage_contract_tests!(
                $make,
                a_blob_reads_back_as_stored,
                a_missing_blob_reads_as_none_and_deleting_it_does_nothing,
                putting_again_replaces_the_blob,
                a_deleted_blob_is_gone,
                listing_gives_the_names_of_all_blobs,
                the_marker_changes_when_the_blob_is_overwritten,
                a_blob_is_not_visible_while_it_is_written,
                a_failed_put_leaves_nothing_behind
            );
        };
        ($make:expr, $($check:ident),+) => {
            $(
                #[test]
                fn $check() {
                    let Some((_alive, storage)) = $make() else {
                        return;
                    };
                    $crate::storage::contract::$check(&storage);
                }
            )+
        };
    }
    pub(crate) use storage_contract_tests;
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::contract::{self, Scratch, storage_contract_tests};
    use super::*;
    use std::sync::{Arc, Mutex};

    fn scratch_storage() -> Option<(Scratch, DirStorage)> {
        let scratch = Scratch::new("storage");
        let storage = DirStorage::open(&scratch.0).unwrap();
        Some((scratch, storage))
    }

    mod dir_storage {
        use super::*;
        storage_contract_tests!(scratch_storage);
    }

    /// A storage that records the files it is asked to have downloaded.
    fn storage_asking_downloads() -> (Scratch, DirStorage, Arc<Mutex<Vec<PathBuf>>>) {
        let scratch = Scratch::new("placeholders");
        let asked = Arc::new(Mutex::new(Vec::new()));
        let log = Arc::clone(&asked);
        let storage = DirStorage::open_downloading(&scratch.0, move |path| {
            log.lock().unwrap().push(path.to_path_buf());
        })
        .unwrap();
        (scratch, storage, asked)
    }

    #[test]
    fn a_placeholder_is_absent_while_its_download_is_requested() {
        let (alive, storage, asked) = storage_asking_downloads();
        fs::write(alive.0.join(".Sketch.excalidraw.icloud"), b"stub").unwrap();

        assert!(storage.list().unwrap().is_empty());
        assert!(get_bytes(&storage, "Sketch.excalidraw").unwrap().is_none());

        let asked = asked.lock().unwrap();
        assert!(!asked.is_empty());
        assert!(
            asked
                .iter()
                .all(|p| p == &alive.0.join("Sketch.excalidraw"))
        );
    }

    #[test]
    fn once_downloaded_a_later_listing_finds_the_file() {
        let (alive, storage, _asked) = storage_asking_downloads();
        let placeholder = alive.0.join(".Sketch.excalidraw.icloud");
        fs::write(&placeholder, b"stub").unwrap();
        assert!(storage.list().unwrap().is_empty());

        // What the system does when the download is done.
        fs::remove_file(&placeholder).unwrap();
        fs::write(alive.0.join("Sketch.excalidraw"), b"{}").unwrap();

        assert_eq!(contract::names(&storage), ["Sketch.excalidraw"]);
        assert_eq!(
            get_bytes(&storage, "Sketch.excalidraw").unwrap().unwrap(),
            b"{}"
        );
    }

    #[test]
    fn deleting_a_placeholder_deletes_it() {
        let (alive, storage, _asked) = storage_asking_downloads();
        let placeholder = alive.0.join(".Sketch.excalidraw.icloud");
        fs::write(&placeholder, b"stub").unwrap();

        storage.delete("Sketch.excalidraw").unwrap();

        assert!(!placeholder.exists());
    }

    #[test]
    fn other_hidden_files_are_neither_listed_nor_downloaded() {
        let (alive, storage, asked) = storage_asking_downloads();
        fs::write(alive.0.join(".DS_Store"), b"x").unwrap();
        fs::write(alive.0.join(".icloud"), b"x").unwrap();

        assert!(storage.list().unwrap().is_empty());
        assert!(asked.lock().unwrap().is_empty());
    }

    #[test]
    fn names_that_could_leave_the_storage_are_invalid_input() {
        let (_alive, storage) = scratch_storage().unwrap();
        for name in ["", "../x", "a/b", "/x", ".incoming", ".hidden", ".."] {
            let put = storage.put(name, &mut &b"x"[..]);
            assert_eq!(
                put.unwrap_err().kind(),
                io::ErrorKind::InvalidInput,
                "{name}"
            );
            assert!(storage.get(name).is_err(), "{name}");
            assert!(storage.delete(name).is_err(), "{name}");
        }
    }

    #[test]
    fn running_out_of_space_is_storage_full_and_leaves_nothing_behind() {
        struct NoSpace;
        impl Read for NoSpace {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::from_raw_os_error(28)) // ENOSPC
            }
        }
        let (_alive, storage) = scratch_storage().unwrap();

        let error = storage.put("blob", &mut NoSpace).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(storage.list().unwrap().is_empty());
    }

    #[test]
    fn opening_discards_puts_that_never_finished() {
        let (alive, storage) = scratch_storage().unwrap();
        fs::write(alive.0.join(INCOMING).join("stale"), b"half").unwrap();
        drop(storage);

        let storage = DirStorage::open(&alive.0).unwrap();

        let incoming = fs::read_dir(alive.0.join(INCOMING)).unwrap();
        assert_eq!(incoming.count(), 0);
        assert!(storage.list().unwrap().is_empty());
    }

    #[test]
    fn opening_keeps_the_puts_this_process_is_writing() {
        let (alive, _first) = scratch_storage().unwrap();
        let writing = alive
            .0
            .join(INCOMING)
            .join(format!("{}-7", std::process::id()));
        fs::write(&writing, b"half").unwrap();

        let _second = DirStorage::open(&alive.0).unwrap();

        assert!(writing.exists());
    }
}
