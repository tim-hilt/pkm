//! The native Sync runner (ADR-0016): answers the requests of the Sync step
//! machine (`step`, where the rules of ADR-0011 live) with file access in a
//! Context's folder and its Sync state folder, and the blocking `Storage`.
//! Syncs any number of Context folders one after the other (ADR-0020).

use std::cell::Cell;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::merge::{Drawings, Merge};
use crate::step::{
    LocalFile, Marks, Next, Progress, Request, Response, StepOutcome, SyncStep, Synced,
};
use crate::storage::{Storage, write_file_atomically};

/// What this Device remembers of each Item of one Context folder at its last
/// Sync, in a folder of its own: never inside the Context folder, never
/// Synced. Forgotten on Disconnect (`forget`).
#[derive(Debug, Clone)]
pub struct SyncState {
    dir: PathBuf,
}

impl SyncState {
    /// The state in `dir`, created if missing.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(dir.join("bytes"))?;
        fs::create_dir_all(dir.join("markers"))?;
        fs::create_dir_all(dir.join("stamps"))?;
        Ok(Self { dir })
    }

    /// Deletes the state in `dir`, so that nothing is remembered: Disconnect.
    pub fn forget(dir: &Path) -> io::Result<()> {
        match fs::remove_dir_all(dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    fn get(&self, file: &str) -> io::Result<Option<Synced>> {
        let read = |kind: &str| match fs::read(self.dir.join(kind).join(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            other => other.map(Some),
        };
        // Either part missing (a crash between the two writes) is no history.
        let (Some(bytes), Some(marker)) = (read("bytes")?, read("markers")?) else {
            return Ok(None);
        };
        Ok(String::from_utf8(marker)
            .ok()
            .map(|marker| Synced { bytes, marker }))
    }

    /// The cloud blob's change marker as last seen, without the bytes.
    fn marker(&self, file: &str) -> io::Result<Option<String>> {
        match fs::read_to_string(self.dir.join("markers").join(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(None),
            other => other.map(Some),
        }
    }

    /// The stamp the local file had when it was read and found equal to the
    /// remembered bytes. Gone as soon as those change.
    fn stamp(&self, file: &str) -> io::Result<Option<String>> {
        match fs::read_to_string(self.dir.join("stamps").join(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) if e.kind() == io::ErrorKind::InvalidData => Ok(None),
            other => other.map(Some),
        }
    }

    /// Remembers that the local file with `stamp` holds the remembered bytes.
    /// `stamp` must have been taken before the read that showed it.
    fn set_stamp(&self, file: &str, stamp: &str) -> io::Result<()> {
        write_file_atomically(&self.dir.join("stamps").join(file), stamp.as_bytes())
    }

    fn remove_stamp(&self, file: &str) -> io::Result<()> {
        match fs::remove_file(self.dir.join("stamps").join(file)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// Remembers `bytes` and `marker`. The marker goes last: without it the
    /// bytes count for nothing. The stamp goes first: it says nothing about
    /// other bytes.
    fn set(&self, file: &str, bytes: &[u8], marker: &str) -> io::Result<()> {
        self.remove_stamp(file)?;
        write_file_atomically(&self.dir.join("bytes").join(file), bytes)?;
        write_file_atomically(&self.dir.join("markers").join(file), marker.as_bytes())
    }

    fn remove(&self, file: &str) -> io::Result<()> {
        for kind in ["stamps", "markers", "bytes"] {
            match fs::remove_file(self.dir.join(kind).join(file)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    fn files(&self) -> io::Result<Vec<String>> {
        Ok(
            files_in(&self.dir.join("markers"), |name| !name.starts_with('.'))?
                .into_iter()
                .map(|(name, _)| name)
                .collect(),
        )
    }
}

/// Which file a local Drawing is, cheaply: its inode, modification time and
/// size. Sync writes files by renaming a new one into place, which gives a new
/// inode, and an edit in place changes the modification time. The text is
/// what the stamps folder holds.
fn stamp_of(meta: &fs::Metadata) -> LocalFile {
    #[cfg(unix)]
    let inode = std::os::unix::fs::MetadataExt::ino(meta);
    #[cfg(not(unix))]
    let inode = 0;
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |t| t.as_nanos());
    LocalFile {
        file: String::new(),
        stamp: format!("{inode} {mtime_ns} {}", meta.len()),
        mtime_ms: (mtime_ns / 1_000_000) as u64,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |t| t.as_millis() as u64)
}

/// One Context folder to Sync: its folder on this Device, what is remembered
/// of it, its cloud folder and its merge hook.
pub struct Folder<'a> {
    pub dir: &'a Path,
    pub state: &'a SyncState,
    pub storage: &'a dyn Storage,
    pub merge: Arc<dyn Merge>,
}

/// Runs one Sync step for each of `folders` in turn, and lists each cloud
/// folder once when nothing changed. Stops as soon as `cancel` is set, also in
/// the middle of a transfer, or once access is lost. Gives the outcome of each
/// folder that ran, in order.
///
/// `progress` counts the transfers of all folders together: each folder adds
/// its own once it knows them, so the total grows as the folders come up.
pub fn sync_folders(
    folders: &[Folder],
    cancel: &AtomicBool,
    progress: &dyn Fn(Progress),
) -> Vec<StepOutcome> {
    let mut outcomes = Vec::new();
    let (mut finished, mut planned) = (0, 0);
    for folder in folders {
        let latest = Cell::new(None);
        let outcome = sync_folder(folder, cancel, &|p| {
            latest.set(Some(p));
            progress(Progress {
                done: finished + p.done,
                total: planned + p.total,
            });
        });
        if let Some(p) = latest.get() {
            finished += p.done;
            planned += p.total;
        }
        let stop = outcome.access_lost || cancel.load(Relaxed);
        outcomes.push(outcome);
        if stop {
            break;
        }
    }
    outcomes
}

/// `sync_folders` for the Drawings folder alone.
pub fn sync_step(
    drawings: &Path,
    state: &SyncState,
    storage: &dyn Storage,
    cancel: &AtomicBool,
) -> StepOutcome {
    sync_step_reporting(drawings, state, storage, cancel, &|_| {})
}

/// `sync_step`, calling `progress` once the number of Drawings to transfer is
/// known and after each one.
pub fn sync_step_reporting(
    drawings: &Path,
    state: &SyncState,
    storage: &dyn Storage,
    cancel: &AtomicBool,
    progress: &dyn Fn(Progress),
) -> StepOutcome {
    let folder = Folder {
        dir: drawings,
        state,
        storage,
        merge: Arc::new(Drawings),
    };
    sync_folders(&[folder], cancel, progress).remove(0)
}

fn sync_folder(folder: &Folder, cancel: &AtomicBool, progress: &dyn Fn(Progress)) -> StepOutcome {
    let runner = Runner {
        dir: folder.dir,
        state: folder.state,
        storage: folder.storage,
        merge: &*folder.merge,
        cancel,
    };
    let mut step = SyncStep::new(now_ms(), Arc::clone(&folder.merge));
    let mut reported = None;
    let mut reply = None;
    loop {
        let next = step.resume(reply.take());
        if step.progress() != reported {
            reported = step.progress();
            if let Some(p) = reported {
                progress(p);
            }
        }
        let Next::Request(request) = next else {
            let Next::Done(outcome) = next else {
                unreachable!()
            };
            return outcome;
        };
        if cancel.load(Relaxed) {
            return step.cancel();
        }
        let result = runner.answer(request);
        // Whatever the error, cancelling is why the transfer failed.
        if result.is_err() && cancel.load(Relaxed) {
            return step.cancel();
        }
        reply = Some(result);
    }
}

struct Runner<'a> {
    dir: &'a Path,
    state: &'a SyncState,
    storage: &'a dyn Storage,
    merge: &'a dyn Merge,
    cancel: &'a AtomicBool,
}

impl Runner<'_> {
    fn answer(&self, request: Request) -> io::Result<Response> {
        Ok(match request {
            Request::ListCloud => Response::Cloud(self.storage.list()?),
            Request::ListLocal => Response::Local(self.list_local()?),
            Request::ReadLocal { file } => Response::Bytes(self.read_local(&file)?),
            Request::ReadMarks { file } => Response::Marks(Marks {
                marker: self.state.marker(&file)?,
                stamp: self.state.stamp(&file)?,
            }),
            Request::ReadSynced { file } => Response::Synced(self.state.get(&file)?),
            Request::ListSynced => Response::Names(self.state.files()?),
            Request::WriteSynced {
                file,
                bytes,
                marker,
            } => {
                self.state.set(&file, &bytes, &marker)?;
                Response::Done
            }
            Request::WriteStamp { file, stamp } => {
                self.state.set_stamp(&file, &stamp)?;
                Response::Done
            }
            Request::ForgetSynced { file } => {
                self.state.remove(&file)?;
                Response::Done
            }
            Request::Download { file } => Response::Bytes(self.download(&file)?),
            Request::Upload { file, bytes } => {
                let mut data = Cancellable {
                    inner: &bytes[..],
                    cancel: self.cancel,
                };
                self.storage.put(&file, &mut data)?;
                Response::Done
            }
            Request::DeleteCloud { file } => {
                self.storage.delete(&file)?;
                Response::Done
            }
            Request::WriteLocal {
                file,
                bytes,
                expected,
            } => Response::Happened(
                if self.read_local(&file)?.as_deref() == expected.as_deref() {
                    write_file_atomically(&self.dir.join(&file), &bytes)?;
                    true
                } else {
                    false
                },
            ),
            Request::RenameLocal { from, to, expected } => Response::Happened(
                if self.read_local(&from)?.as_deref() == Some(&expected[..]) {
                    fs::rename(self.dir.join(&from), self.dir.join(&to))?;
                    true
                } else {
                    false
                },
            ),
            Request::DeleteLocal { file, expected } => Response::Happened(
                if self.read_local(&file)?.as_deref() == Some(&expected[..]) {
                    fs::remove_file(self.dir.join(&file))?;
                    true
                } else {
                    false
                },
            ),
        })
    }

    fn list_local(&self) -> io::Result<Vec<LocalFile>> {
        Ok(files_in(self.dir, |name| self.merge.accepts(name))?
            .into_iter()
            .map(|(file, meta)| LocalFile {
                file,
                ..stamp_of(&meta)
            })
            .collect())
    }

    fn read_local(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.dir.join(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            // Not a file (a folder called so): not an Item.
            Err(e) if e.kind() == io::ErrorKind::IsADirectory => Ok(None),
            other => other.map(Some),
        }
    }

    /// The cloud's Drawing, if it is still there.
    fn download(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(data) = self.storage.get(file)? else {
            return Ok(None);
        };
        let mut bytes = Vec::new();
        Cancellable {
            inner: data,
            cancel: self.cancel,
        }
        .read_to_end(&mut bytes)?;
        Ok(Some(bytes))
    }
}

/// A reader that fails once `cancel` is set, so that a running transfer stops.
struct Cancellable<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Relaxed) {
            return Err(io::Error::other("cancelled"));
        }
        self.inner.read(buf)
    }
}

/// The files in `dir` that `wanted` accepts by name, with their metadata,
/// from one scan.
fn files_in(dir: &Path, wanted: impl Fn(&str) -> bool) -> io::Result<Vec<(String, fs::Metadata)>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if wanted(&name) {
            let meta = entry.metadata()?;
            if meta.is_file() {
                files.push((name, meta));
            }
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    mod problems {
        //! What a Sync step reports: progress through the transfers, and the two
        //! problems that block Sync: lost access and a full cloud.

        use std::fs;
        use std::io::{self, Read};
        use std::path::PathBuf;
        use std::sync::Mutex;
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};

        use crate::{
            Blob, DirStorage, Progress, StepOutcome, Storage, SyncState, sync_step_reporting,
        };

        struct Scratch(PathBuf);

        impl Scratch {
            fn new(prefix: &str) -> Self {
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let dir = std::env::temp_dir().join(format!(
                    "pkm-problems-{prefix}-{}-{}",
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

        /// A directory storage that can be made to fail.
        struct Flaky {
            inner: DirStorage,
            /// What `put` fails with, while set.
            put_fails: Mutex<Option<io::ErrorKind>>,
            /// What every call fails with, while set.
            all_fail: Mutex<Option<io::ErrorKind>>,
        }

        impl Flaky {
            fn check(&self, slot: &Mutex<Option<io::ErrorKind>>) -> io::Result<()> {
                match *slot.lock().unwrap() {
                    Some(kind) => Err(io::Error::from(kind)),
                    None => Ok(()),
                }
            }
        }

        impl Storage for Flaky {
            fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
                self.check(&self.all_fail)?;
                self.check(&self.put_fails)?;
                self.inner.put(name, data)
            }
            fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
                self.check(&self.all_fail)?;
                self.inner.get(name)
            }
            fn list(&self) -> io::Result<Vec<Blob>> {
                self.check(&self.all_fail)?;
                self.inner.list()
            }
            fn delete(&self, name: &str) -> io::Result<()> {
                self.check(&self.all_fail)?;
                self.inner.delete(name)
            }
        }

        struct World {
            cloud: Scratch,
            drawings: Scratch,
            state: Scratch,
            storage: Flaky,
        }

        impl World {
            fn new() -> Self {
                let cloud = Scratch::new("cloud");
                let storage = Flaky {
                    inner: DirStorage::open(&cloud.0).unwrap(),
                    put_fails: Mutex::default(),
                    all_fail: Mutex::default(),
                };
                World {
                    cloud,
                    drawings: Scratch::new("drawings"),
                    state: Scratch::new("state"),
                    storage,
                }
            }

            fn local(&self, file: &str, bytes: &[u8]) {
                fs::write(self.drawings.0.join(file), bytes).unwrap();
            }

            fn cloud(&self, file: &str, bytes: &[u8]) {
                fs::write(self.cloud.0.join(file), bytes).unwrap();
            }

            fn step(&self) -> (StepOutcome, Vec<(u32, u32)>) {
                let reports = Mutex::new(Vec::new());
                let outcome = sync_step_reporting(
                    &self.drawings.0,
                    &SyncState::open(&self.state.0).unwrap(),
                    &self.storage,
                    &AtomicBool::new(false),
                    &|p: Progress| reports.lock().unwrap().push((p.done, p.total)),
                );
                (outcome, reports.into_inner().unwrap())
            }
        }

        /// A Drawing that merges fine: the smallest valid file.
        const DRAWING: &[u8] =
            br#"{"type":"excalidraw","version":2,"elements":[],"appState":{},"files":{}}"#;

        #[test]
        fn progress_counts_the_drawings_done_of_all_to_transfer() {
            let w = World::new();
            w.local("Up.excalidraw", DRAWING);
            w.cloud("Down.excalidraw", DRAWING);

            let (outcome, reports) = w.step();

            assert!(outcome.error.is_none());
            assert_eq!(reports, [(0, 2), (1, 2), (2, 2)]);
        }

        #[test]
        fn nothing_to_transfer_reports_no_progress() {
            let w = World::new();
            w.local("A.excalidraw", DRAWING);
            w.step();

            let (_, reports) = w.step();

            assert_eq!(reports, []);
        }

        #[test]
        fn deletes_are_exchanges_of_state_and_report_no_progress() {
            let w = World::new();
            w.local("A.excalidraw", DRAWING);
            w.step();
            fs::remove_file(w.drawings.0.join("A.excalidraw")).unwrap();

            let (_, reports) = w.step();

            assert_eq!(reports, []);
            assert!(!w.cloud.0.join("A.excalidraw").exists());
        }

        #[test]
        fn rejected_credentials_are_access_lost() {
            let w = World::new();
            w.local("A.excalidraw", DRAWING);
            *w.storage.all_fail.lock().unwrap() = Some(io::ErrorKind::PermissionDenied);

            let (outcome, _) = w.step();

            assert!(outcome.access_lost);
        }

        #[test]
        fn a_full_cloud_blocks_uploads_but_not_downloads() {
            let w = World::new();
            w.local("Up.excalidraw", DRAWING);
            w.cloud("Down.excalidraw", DRAWING);
            *w.storage.put_fails.lock().unwrap() = Some(io::ErrorKind::StorageFull);

            let (outcome, _) = w.step();

            assert!(outcome.storage_full);
            assert!(!outcome.access_lost);
            assert!(outcome.error.is_none(), "full is not an offline failure");
            assert!(w.drawings.0.join("Down.excalidraw").exists());
            assert!(!w.cloud.0.join("Up.excalidraw").exists());
        }

        #[test]
        fn a_full_cloud_clears_itself_once_an_upload_succeeds() {
            let w = World::new();
            w.local("Up.excalidraw", DRAWING);
            *w.storage.put_fails.lock().unwrap() = Some(io::ErrorKind::StorageFull);
            assert!(w.step().0.storage_full);

            *w.storage.put_fails.lock().unwrap() = None;
            let (outcome, _) = w.step();

            assert!(!outcome.storage_full);
            assert!(w.cloud.0.join("Up.excalidraw").exists());
        }

        #[test]
        fn other_failures_are_errors_and_no_problem() {
            let w = World::new();
            *w.storage.all_fail.lock().unwrap() = Some(io::ErrorKind::ConnectionRefused);

            let (outcome, _) = w.step();

            assert!(outcome.error.is_some());
            assert!(!outcome.access_lost && !outcome.storage_full);
        }
    }

    mod sync {
        //! Sync steps of several Devices (temporary folders) against one shared
        //! directory storage standing in for the cloud.

        use std::fs;
        use std::io::{self, Read};
        use std::path::{Path, PathBuf};
        use std::sync::atomic::{AtomicBool, AtomicU32, Ordering::Relaxed};
        use std::sync::{Arc, Mutex};

        use crate::{
            Blob, BookStates, Books as BookFiles, DirStorage, Drawings, Filters, Folder, Merge,
            Progress, Storage, SyncState, TagsFile, Tasks, sync_folders, sync_step,
        };
        use pkm_base::json;
        use pkm_books::library::Library;

        struct Scratch(PathBuf);

        impl Scratch {
            fn new(prefix: &str) -> Self {
                static NEXT: AtomicU32 = AtomicU32::new(0);
                let dir = std::env::temp_dir().join(format!(
                    "pkm-sync-{prefix}-{}-{}",
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

        /// The calls a storage got.
        #[derive(Default)]
        struct Calls {
            lists: u32,
            gets: Vec<String>,
            puts: Vec<String>,
            deletes: Vec<String>,
        }

        struct Counting {
            inner: DirStorage,
            calls: Mutex<Calls>,
            /// Another Device's write that lands right after the next `put`.
            overwrite_after_put: Mutex<Option<Vec<u8>>>,
        }

        impl Storage for Counting {
            fn put(&self, name: &str, data: &mut dyn Read) -> io::Result<()> {
                self.calls.lock().unwrap().puts.push(name.into());
                self.inner.put(name, data)?;
                if let Some(other) = self.overwrite_after_put.lock().unwrap().take() {
                    self.inner.put(name, &mut &other[..])?;
                }
                Ok(())
            }
            fn get(&self, name: &str) -> io::Result<Option<Box<dyn Read + Send>>> {
                self.calls.lock().unwrap().gets.push(name.into());
                self.inner.get(name)
            }
            fn list(&self) -> io::Result<Vec<Blob>> {
                self.calls.lock().unwrap().lists += 1;
                self.inner.list()
            }
            fn delete(&self, name: &str) -> io::Result<()> {
                self.calls.lock().unwrap().deletes.push(name.into());
                self.inner.delete(name)
            }
        }

        /// The shared cloud folder.
        struct Cloud {
            dir: Scratch,
            storage: Counting,
        }

        impl Cloud {
            fn new() -> Self {
                let dir = Scratch::new("cloud");
                let storage = Counting {
                    inner: DirStorage::open(&dir.0).unwrap(),
                    calls: Mutex::default(),
                    overwrite_after_put: Mutex::default(),
                };
                Cloud { dir, storage }
            }

            /// Edits the cloud folder directly, as a server user would.
            fn write(&self, file: &str, bytes: &[u8]) {
                fs::write(self.dir.0.join(file), bytes).unwrap();
            }

            fn remove(&self, file: &str) {
                fs::remove_file(self.dir.0.join(file)).unwrap();
            }

            fn read(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.dir.0.join(file)).ok()
            }

            /// The Drawings in the cloud folder, not the storage's own bookkeeping.
            fn files(&self) -> Vec<String> {
                listing(&self.dir.0)
                    .into_iter()
                    .filter(|name| !name.starts_with('.'))
                    .collect()
            }

            /// The next upload is overwritten with `bytes` before anything else
            /// happens, as by another Device.
            fn overwrite_next_upload(&self, bytes: Vec<u8>) {
                *self.storage.overwrite_after_put.lock().unwrap() = Some(bytes);
            }

            fn take_calls(&self) -> Calls {
                std::mem::take(&mut *self.storage.calls.lock().unwrap())
            }
        }

        /// A Device: its Drawings folder and its Sync state folder.
        struct Device {
            drawings: Scratch,
            state: Scratch,
            /// A second Context folder, for the tests of several (`sync_both`).
            notes: Notes,
            /// The Books Context: Book Files, with their state hidden in `.state`.
            books: Scratch,
            /// The Tasks Context: Task files.
            tasks: Scratch,
        }

        impl Device {
            fn new() -> Self {
                let books = Scratch::new("books");
                fs::create_dir_all(books.0.join(".state")).unwrap();
                let tasks = Scratch::new("tasks");
                Device {
                    tasks,
                    drawings: Scratch::new("drawings"),
                    state: Scratch::new("state"),
                    notes: Notes::new(),
                    books,
                }
            }

            fn write(&self, file: &str, bytes: &[u8]) {
                fs::write(self.drawings.0.join(file), bytes).unwrap();
            }

            /// Dates the file an hour back: a file that was left alone for a while.
            fn settle(&self, file: &str) {
                let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
                fs::File::options()
                    .write(true)
                    .open(self.drawings.0.join(file))
                    .unwrap()
                    .set_modified(hour_ago)
                    .unwrap();
            }

            fn remove(&self, file: &str) {
                fs::remove_file(self.drawings.0.join(file)).unwrap();
            }

            fn rename(&self, from: &str, to: &str) {
                fs::rename(self.drawings.0.join(from), self.drawings.0.join(to)).unwrap();
            }

            fn read(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.drawings.0.join(file)).ok()
            }

            fn files(&self) -> Vec<String> {
                listing(&self.drawings.0)
            }

            /// Writes a Book File into the Books folder.
            fn write_book(&self, file: &str, bytes: &[u8]) {
                fs::write(self.books.0.join(file), bytes).unwrap();
            }

            fn read_book(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.books.0.join(file)).ok()
            }

            fn remove_book(&self, file: &str) {
                fs::remove_file(self.books.0.join(file)).unwrap();
            }

            /// Writes a state file into the hidden `.state` beside the Book Files.
            fn write_book_state(&self, file: &str, bytes: &[u8]) {
                fs::write(self.books.0.join(".state").join(file), bytes).unwrap();
            }

            fn read_book_state(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.books.0.join(".state").join(file)).ok()
            }

            fn remove_book_state(&self, file: &str) {
                fs::remove_file(self.books.0.join(".state").join(file)).unwrap();
            }

            fn book_files(&self) -> Vec<String> {
                listing(&self.books.0)
                    .into_iter()
                    .filter(|name| !name.starts_with('.'))
                    .collect()
            }

            fn write_task(&self, file: &str, bytes: &[u8]) {
                fs::write(self.tasks.0.join(file), bytes).unwrap();
            }

            fn read_task(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.tasks.0.join(file)).ok()
            }

            fn remove_task(&self, file: &str) {
                fs::remove_file(self.tasks.0.join(file)).unwrap();
            }

            /// One Sync step over the Tasks Context. Returns the names it changed here.
            fn sync_tasks(&self, cloud: &Cloud) -> Vec<String> {
                let tasks_state = SyncState::open(self.state.0.join("Tasks")).unwrap();
                let folders = [Folder {
                    dir: &self.tasks.0,
                    state: &tasks_state,
                    storage: &cloud.storage,
                    merge: Arc::new(Tasks),
                }];
                let outcomes = sync_folders(&folders, &AtomicBool::new(false), &|_| {});
                let mut changed = Vec::new();
                for outcome in outcomes {
                    assert!(outcome.error.is_none(), "{:?}", outcome.error);
                    changed.extend(outcome.changed);
                }
                changed.sort();
                changed
            }

            /// One Sync step over the Books Context: the Book Files and their state
            /// files, each with its own history. Returns the names it changed here.
            fn sync_books(&self, cloud: &Cloud) -> Vec<String> {
                let files_state = SyncState::open(self.state.0.join("Books")).unwrap();
                let states_state = SyncState::open(self.state.0.join("BooksState")).unwrap();
                let folders = [
                    Folder {
                        dir: &self.books.0,
                        state: &files_state,
                        storage: &cloud.storage,
                        merge: Arc::new(BookFiles),
                    },
                    Folder {
                        dir: &self.books.0.join(".state"),
                        state: &states_state,
                        storage: &cloud.storage,
                        merge: Arc::new(BookStates),
                    },
                ];
                let outcomes = sync_folders(&folders, &AtomicBool::new(false), &|_| {});
                assert_eq!(outcomes.len(), 2);
                for outcome in &outcomes {
                    assert!(outcome.error.is_none(), "{:?}", outcome.error);
                    assert!(!outcome.access_lost && !outcome.storage_full);
                }
                let mut changed: Vec<String> =
                    outcomes.into_iter().flat_map(|o| o.changed).collect();
                changed.sort();
                changed
            }

            /// One Sync step; returns the names it changed here.
            fn sync(&self, cloud: &Cloud) -> Vec<String> {
                let state = SyncState::open(&self.state.0).unwrap();
                let outcome = sync_step(
                    &self.drawings.0,
                    &state,
                    &cloud.storage,
                    &AtomicBool::new(false),
                );
                assert!(outcome.error.is_none(), "{:?}", outcome.error);
                // A file that can't be read is `PermissionDenied`, as lost access is.
                assert!(!outcome.access_lost && !outcome.storage_full);
                let mut changed = outcome.changed;
                changed.sort();
                changed
            }
        }

        fn listing(dir: &Path) -> Vec<String> {
            let mut names: Vec<_> = fs::read_dir(dir)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }

        #[test]
        fn a_new_drawing_reaches_the_cloud_and_other_devices() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");

            assert!(phone.sync(&cloud).is_empty());
            assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"one");
            assert_eq!(tablet.sync(&cloud), ["Sketch"]);
            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"one");
        }

        #[test]
        fn a_changed_drawing_reaches_other_devices() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            phone.write("Sketch.excalidraw", b"two!");
            phone.sync(&cloud);

            assert_eq!(tablet.sync(&cloud), ["Sketch"]);
            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"two!");
        }

        #[test]
        fn a_deleted_drawing_is_deleted_on_other_devices() {
            let cloud = Cloud::new();
            let (phone, tablet, laptop) = (Device::new(), Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);
            laptop.sync(&cloud);

            phone.remove("Sketch.excalidraw");
            phone.sync(&cloud);

            assert!(cloud.read("Sketch.excalidraw").is_none());
            assert_eq!(tablet.sync(&cloud), ["Sketch"]);
            assert_eq!(laptop.sync(&cloud), ["Sketch"]);
            assert!(tablet.files().is_empty() && laptop.files().is_empty());
        }

        #[test]
        fn a_rename_is_a_delete_of_the_old_name_and_an_add_of_the_new_one() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Old.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            phone.rename("Old.excalidraw", "New.excalidraw");
            phone.sync(&cloud);

            assert_eq!(tablet.sync(&cloud), ["New", "Old"]);
            assert_eq!(tablet.files(), ["New.excalidraw"]);
            assert_eq!(tablet.read("New.excalidraw").unwrap(), b"one");
        }

        #[test]
        fn a_duplicate_is_a_new_drawing() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            phone.write("Sketch copy.excalidraw", b"one");
            phone.sync(&cloud);

            assert_eq!(tablet.sync(&cloud), ["Sketch copy"]);
            assert_eq!(
                tablet.files(),
                ["Sketch copy.excalidraw", "Sketch.excalidraw"]
            );
        }

        #[test]
        fn edits_made_directly_in_either_folder_are_synced_like_the_apps_own() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("A.excalidraw", b"a");
            phone.write("B.excalidraw", b"b");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            // On the server: edit A, remove B, add C. In Files on the tablet: add D.
            cloud.write("A.excalidraw", b"a, edited");
            cloud.remove("B.excalidraw");
            cloud.write("C.excalidraw", b"c");
            tablet.write("D.excalidraw", b"d");
            tablet.sync(&cloud);

            assert_eq!(
                tablet.files(),
                ["A.excalidraw", "C.excalidraw", "D.excalidraw"]
            );
            assert_eq!(tablet.read("A.excalidraw").unwrap(), b"a, edited");
            phone.sync(&cloud);
            assert_eq!(phone.files(), tablet.files());
        }

        #[test]
        fn files_that_are_no_drawings_are_ignored() {
            let cloud = Cloud::new();
            let phone = Device::new();
            phone.write("notes.txt", b"x");
            cloud.write("readme.md", b"y");
            phone.sync(&cloud);

            assert_eq!(phone.files(), ["notes.txt"]);
            assert!(cloud.read("notes.txt").is_none());
        }

        #[test]
        fn nothing_half_written_is_left_on_either_side() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", &vec![b'x'; 1 << 20]);
            phone.sync(&cloud);
            tablet.sync(&cloud);

            assert_eq!(tablet.files(), ["Sketch.excalidraw"]);
            assert_eq!(tablet.read("Sketch.excalidraw").unwrap().len(), 1 << 20);
        }

        #[test]
        fn unknown_content_is_preserved_byte_for_byte() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            let bytes = "{\"type\":\"excalidraw\",\"elements\":[{\"type\":\"hologram\",\"x\":1.50,\"glow\":[1,2]}],\"zzz\":\"\u{1F600}\"}\n  ";
            phone.write("Sketch.excalidraw", bytes.as_bytes());
            phone.sync(&cloud);
            tablet.sync(&cloud);

            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), bytes.as_bytes());
        }

        #[test]
        fn an_idle_poll_is_one_listing_and_transfers_nothing() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);
            cloud.take_calls();

            phone.sync(&cloud);
            tablet.sync(&cloud);

            let calls = cloud.take_calls();
            assert_eq!(calls.lists, 2);
            assert!(calls.gets.is_empty() && calls.puts.is_empty() && calls.deletes.is_empty());
        }

        #[cfg(unix)]
        #[test]
        fn an_idle_poll_does_not_read_the_drawings() {
            use std::os::unix::fs::PermissionsExt;

            let cloud = Cloud::new();
            let phone = Device::new();
            phone.write("Sketch.excalidraw", b"one");
            phone.settle("Sketch.excalidraw");
            phone.sync(&cloud); // uploads
            phone.sync(&cloud); // reads it again, finds it unchanged, remembers its stamp

            // What can't be read can only be found unchanged without reading it.
            let path = phone.drawings.0.join("Sketch.excalidraw");
            fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).unwrap();
            if fs::read(&path).is_ok() {
                return; // running as root: permissions don't keep anything out
            }
            let changed = phone.sync(&cloud);
            fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

            assert!(changed.is_empty());
        }

        #[test]
        fn a_drawing_edited_after_polls_without_reading_is_still_synced() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.settle("Sketch.excalidraw");
            phone.sync(&cloud);
            phone.sync(&cloud);
            phone.sync(&cloud);

            // Same size, edited in place.
            let path = phone.drawings.0.join("Sketch.excalidraw");
            let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
            io::Write::write_all(&mut &file, b"two").unwrap();
            drop(file);
            phone.sync(&cloud);
            tablet.sync(&cloud);

            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"two");
        }

        #[test]
        fn a_drawing_replaced_by_another_file_of_the_same_size_is_still_synced() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.settle("Sketch.excalidraw");
            phone.sync(&cloud);
            phone.sync(&cloud);
            phone.sync(&cloud);

            // As an editor that saves by renaming a new file over it does.
            phone.write("Other.excalidraw", b"two");
            phone.settle("Other.excalidraw");
            phone.rename("Other.excalidraw", "Sketch.excalidraw");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"two");
        }

        #[test]
        fn a_cloud_change_is_found_without_reading_the_local_drawing() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.settle("Sketch.excalidraw");
            phone.sync(&cloud);
            phone.sync(&cloud);
            tablet.sync(&cloud);

            cloud.write("Sketch.excalidraw", b"two");
            phone.sync(&cloud);

            assert_eq!(phone.read("Sketch.excalidraw").unwrap(), b"two");
        }

        #[test]
        fn only_changed_drawings_are_transferred() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            for name in ["A", "B", "C"] {
                phone.write(&format!("{name}.excalidraw"), name.as_bytes());
            }
            phone.sync(&cloud);
            tablet.sync(&cloud);
            cloud.take_calls();

            phone.write("B.excalidraw", b"B, changed");
            phone.sync(&cloud);
            let calls = cloud.take_calls();
            assert_eq!(calls.puts, ["B.excalidraw"]);
            // Only what was uploaded is read back, to see that nobody overwrote it.
            assert_eq!(calls.gets, ["B.excalidraw"]);

            tablet.sync(&cloud);
            let calls = cloud.take_calls();
            assert_eq!(calls.gets, ["B.excalidraw"]);
            assert!(calls.puts.is_empty());
        }

        #[test]
        fn a_drawing_that_cant_be_parsed_is_skipped_and_left_as_it_is_on_both_sides() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            phone.write("Sketch.excalidraw", b"phone");
            cloud.write("Sketch.excalidraw", b"cloud!");
            assert!(phone.sync(&cloud).is_empty());
            assert_eq!(phone.read("Sketch.excalidraw").unwrap(), b"phone");
            assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"cloud!");

            // Also when only the one side is no Drawing.
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Fine.excalidraw", &drawing(&[element("a", 1, "a0", 0)]));
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.write("Fine.excalidraw", &drawing(&[element("a", 2, "a0", 5)]));
            cloud.write("Fine.excalidraw", b"{broken");
            let before = phone.read("Fine.excalidraw").unwrap();
            assert!(phone.sync(&cloud).is_empty());
            assert_eq!(phone.read("Fine.excalidraw").unwrap(), before);
            assert_eq!(cloud.read("Fine.excalidraw").unwrap(), b"{broken");
            // The cloud is fine, the local file is broken.
            cloud.write("Fine.excalidraw", &before);
            tablet.write("Fine.excalidraw", b"{broken");
            tablet.sync(&cloud);
            assert_eq!(tablet.read("Fine.excalidraw").unwrap(), b"{broken");
            assert_eq!(cloud.read("Fine.excalidraw").unwrap(), before);
        }

        // --- Concurrent changes -----------------------------------------------------

        fn element(id: &str, version: u32, index: &str, x: u32) -> String {
            format!(
                r#"{{"id":"{id}","type":"rectangle","version":{version},"versionNonce":7,"index":"{index}","isDeleted":false,"x":{x}}}"#
            )
        }

        fn erased(id: &str, version: u32, index: &str) -> String {
            element(id, version, index, 0).replace("false", "true")
        }

        fn drawing(elements: &[String]) -> Vec<u8> {
            with(elements, "{}", "{}")
        }

        fn with(elements: &[String], app_state: &str, files: &str) -> Vec<u8> {
            format!(
        r#"{{"type":"excalidraw","version":2,"elements":[{}],"appState":{app_state},"files":{files}}}"#,
        elements.join(",")
    )
    .into_bytes()
        }

        /// `(id, version, isDeleted)` of the Elements of a Drawing's bytes, in order.
        fn elements_of(bytes: &[u8]) -> Vec<(String, u64, bool)> {
            let value = json::parse(std::str::from_utf8(bytes).unwrap()).unwrap();
            value
                .get("elements")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|e| {
                    (
                        e.get("id").unwrap().as_str().unwrap().to_owned(),
                        e.get("version").unwrap().as_f64().unwrap() as u64,
                        e.get("isDeleted").unwrap().as_bool().unwrap(),
                    )
                })
                .collect()
        }

        fn e(id: &str, version: u64, deleted: bool) -> (String, u64, bool) {
            (id.into(), version, deleted)
        }

        /// Two Devices that Synced one Drawing of Elements `a` and `b`, then went apart.
        fn two_devices_apart() -> (Cloud, Device, Device) {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 1, "a1", 0)]),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);
            (cloud, phone, tablet)
        }

        #[test]
        fn changes_made_on_two_devices_are_merged_on_every_device() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
            );
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 3, "a1", 30)]),
            );

            phone.sync(&cloud); // reaches the cloud first
            assert!(tablet.sync(&cloud).contains(&"Sketch".to_owned())); // merges, uploads
            assert_eq!(phone.sync(&cloud), ["Sketch"]);

            let merged = [e("a", 2, false), e("b", 3, false)];
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                phone.read("Sketch.excalidraw"),
                cloud.read("Sketch.excalidraw")
            );
            assert_eq!(
                tablet.read("Sketch.excalidraw"),
                cloud.read("Sketch.excalidraw")
            );
        }

        #[test]
        fn the_merge_is_the_last_synced_state() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write("Sketch.excalidraw", &drawing(&[element("a", 2, "a0", 10)]));
            tablet.write("Sketch.excalidraw", &drawing(&[element("b", 2, "a1", 20)]));
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.sync(&cloud);
            cloud.take_calls();

            assert!(tablet.sync(&cloud).is_empty());
            assert!(phone.sync(&cloud).is_empty());
            let calls = cloud.take_calls();
            assert!(
                calls.gets.is_empty() && calls.puts.is_empty(),
                "idle after merge"
            );
        }

        #[test]
        fn an_erase_on_one_side_and_edits_on_the_other_both_survive() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[erased("a", 2, "a0"), element("b", 1, "a1", 0)]),
            );
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 5)]),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.sync(&cloud);

            let merged = [e("a", 2, true), e("b", 2, false)];
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
                merged
            );
        }

        #[test]
        fn images_added_on_two_devices_are_both_kept() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write(
                "Sketch.excalidraw",
                &with(&[element("a", 1, "a0", 0)], "{}", r#"{"f1":{"id":"f1"}}"#),
            );
            tablet.write(
                "Sketch.excalidraw",
                &with(&[element("a", 1, "a0", 0)], "{}", r#"{"f2":{"id":"f2"}}"#),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.sync(&cloud);

            for bytes in [
                phone.read("Sketch.excalidraw").unwrap(),
                cloud.read("Sketch.excalidraw").unwrap(),
            ] {
                let value = json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                assert_eq!(value.get("files").unwrap().as_object().unwrap().len(), 2);
            }
        }

        #[test]
        fn app_state_that_another_tool_stored_survives_a_device_that_never_touched_it() {
            let (cloud, phone, tablet) = two_devices_apart();
            // excalidraw.com sets the background; the phone only edits an Element.
            cloud.write(
                "Sketch.excalidraw",
                &with(
                    &[element("a", 1, "a0", 0), element("b", 1, "a1", 0)],
                    r##"{"viewBackgroundColor":"#000000"}"##,
                    "{}",
                ),
            );
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 9), element("b", 1, "a1", 0)]),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);

            for bytes in [
                phone.read("Sketch.excalidraw").unwrap(),
                tablet.read("Sketch.excalidraw").unwrap(),
            ] {
                let value = json::parse(std::str::from_utf8(&bytes).unwrap()).unwrap();
                let state = value.get("appState").unwrap();
                assert_eq!(
                    state.get("viewBackgroundColor").unwrap().as_str(),
                    Some("#000000")
                );
                assert_eq!(elements_of(&bytes)[0], e("a", 2, false));
            }
        }

        #[test]
        fn an_upload_that_another_device_overwrote_is_written_again_and_all_devices_converge() {
            let (cloud, phone, tablet) = two_devices_apart();
            // Both go to upload at the same time; the tablet's write lands last.
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
            );
            phone.sync(&cloud);
            cloud.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 20)]),
            );

            phone.sync(&cloud); // sees the overwrite, writes its newer Element again
            tablet.sync(&cloud);
            phone.sync(&cloud);

            let merged = [e("a", 2, false), e("b", 2, false)];
            assert_eq!(
                elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&tablet.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                phone.read("Sketch.excalidraw"),
                tablet.read("Sketch.excalidraw")
            );
        }

        #[test]
        fn an_upload_overwritten_before_the_listing_is_still_repaired() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
            );
            // The tablet's write lands right after the phone's upload, before the
            // phone could list the cloud folder.
            cloud.overwrite_next_upload(drawing(&[
                element("a", 1, "a0", 0),
                element("b", 2, "a1", 20),
            ]));
            phone.sync(&cloud);

            phone.sync(&cloud); // sees the overwrite, writes its newer Element again
            tablet.sync(&cloud);
            phone.sync(&cloud);

            let merged = [e("a", 2, false), e("b", 2, false)];
            for bytes in [
                cloud.read("Sketch.excalidraw").unwrap(),
                phone.read("Sketch.excalidraw").unwrap(),
                tablet.read("Sketch.excalidraw").unwrap(),
            ] {
                assert_eq!(elements_of(&bytes), merged);
            }
            assert_eq!(
                phone.read("Sketch.excalidraw"),
                tablet.read("Sketch.excalidraw")
            );
        }

        #[test]
        fn a_local_save_that_lacks_newer_versions_the_cloud_holds_does_not_take_them_back() {
            // The open Drawing kept its own version of `a` while the user dragged it
            // (or its autosave raced Sync's write), and saved over the newer one.
            let (cloud, phone, tablet) = two_devices_apart();
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 5, "a0", 50), element("b", 1, "a1", 0)]),
            );
            tablet.sync(&cloud);
            phone.sync(&cloud);
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 9), element("b", 2, "a1", 7)]),
            );

            phone.sync(&cloud);

            let merged = [e("a", 5, false), e("b", 2, false)];
            assert_eq!(
                elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
                merged
            );
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                merged
            );
        }

        #[test]
        fn a_file_that_cant_be_parsed_never_replaces_a_drawing_on_the_other_side() {
            let (cloud, phone, tablet) = two_devices_apart();
            let before = phone.read("Sketch.excalidraw").unwrap();
            cloud.write("Sketch.excalidraw", b"{broken");

            assert!(phone.sync(&cloud).is_empty());
            assert_eq!(phone.read("Sketch.excalidraw").unwrap(), before);
            assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), b"{broken");

            // The other way round: broken here, the cloud unchanged.
            cloud.write("Sketch.excalidraw", &before);
            phone.sync(&cloud);
            tablet.sync(&cloud);
            tablet.write("Sketch.excalidraw", b"{broken");
            tablet.sync(&cloud);
            assert_eq!(cloud.read("Sketch.excalidraw").unwrap(), before);
            assert_eq!(tablet.read("Sketch.excalidraw").unwrap(), b"{broken");
        }

        #[test]
        fn a_one_sided_change_stays_byte_for_byte_as_the_other_tool_wrote_it() {
            let (cloud, phone, _) = two_devices_apart();
            let theirs = br#"{ "type": "excalidraw", "elements": [ {"id":"a","version":5,"versionNonce":1,"index":"a0","isDeleted":false},{"id":"b","type":"rectangle","version":1,"versionNonce":7,"index":"a1","isDeleted":false,"x":0} ], "appState": {"z":1,"a":2}, "files": {} }"#;
            cloud.write("Sketch.excalidraw", theirs);
            phone.sync(&cloud);
            assert_eq!(phone.read("Sketch.excalidraw").unwrap(), theirs);
        }

        #[test]
        fn a_delete_wins_over_an_edit_either_way_round() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            tablet.sync(&cloud);

            phone.remove("Sketch.excalidraw");
            tablet.write("Sketch.excalidraw", b"edited");
            phone.sync(&cloud); // the delete reaches the cloud
            assert!(cloud.read("Sketch.excalidraw").is_none());
            assert_eq!(tablet.sync(&cloud), ["Sketch"]); // and the edited copy
            assert!(tablet.files().is_empty());
            assert!(phone.sync(&cloud).is_empty());
            assert!(cloud.read("Sketch.excalidraw").is_none());

            // The other way round: edited in the cloud, deleted here.
            phone.write("Sketch.excalidraw", b"two");
            phone.sync(&cloud);
            phone.remove("Sketch.excalidraw");
            cloud.write("Sketch.excalidraw", b"edited again");
            phone.sync(&cloud);
            assert!(phone.files().is_empty());
            assert!(cloud.read("Sketch.excalidraw").is_none());
        }

        #[test]
        fn sync_state_stays_out_of_the_drawings_and_is_forgotten_on_disconnect() {
            let cloud = Cloud::new();
            let phone = Device::new();
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
            assert!(!listing(&phone.state.0).is_empty());

            SyncState::forget(&phone.state.0).unwrap();
            assert!(!phone.state.0.exists());

            // Without history a Drawing deleted meanwhile comes back from the cloud.
            phone.remove("Sketch.excalidraw");
            phone.sync(&cloud);
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
        }

        #[test]
        fn a_cancelled_step_changes_nothing_and_says_so() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);

            let state = SyncState::open(&tablet.state.0).unwrap();
            let outcome = sync_step(
                &tablet.drawings.0,
                &state,
                &cloud.storage,
                &AtomicBool::new(true),
            );
            assert_eq!(outcome.error.unwrap().kind(), io::ErrorKind::Interrupted);
            assert!(tablet.files().is_empty());
        }

        // --- First Connect, same names, deletes -------------------------------------

        fn ids(bytes: &[u8]) -> Vec<String> {
            elements_of(bytes).into_iter().map(|e| e.0).collect()
        }

        #[test]
        fn first_connect_copies_drawings_only_one_side_has() {
            let cloud = Cloud::new();
            let phone = Device::new();
            cloud.write("Cloudy.excalidraw", &drawing(&[element("c", 1, "a0", 0)]));
            phone.write("Local.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
            assert_eq!(phone.sync(&cloud), ["Cloudy"]);
            assert_eq!(phone.files(), ["Cloudy.excalidraw", "Local.excalidraw"]);
            assert!(cloud.read("Local.excalidraw").is_some());
        }

        #[test]
        fn same_named_related_drawings_are_merged_without_a_base() {
            let cloud = Cloud::new();
            let phone = Device::new();
            cloud.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 3, "a1", 0)]),
            );
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 5), element("c", 1, "a2", 0)]),
            );
            phone.sync(&cloud);
            let expected = vec![e("a", 2, false), e("b", 3, false), e("c", 1, false)];
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                expected
            );
            assert_eq!(
                elements_of(&cloud.read("Sketch.excalidraw").unwrap()),
                expected
            );
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
        }

        #[test]
        fn same_named_unrelated_drawings_are_both_kept_and_the_local_one_renamed() {
            let cloud = Cloud::new();
            let phone = Device::new();
            cloud.write("Sketch.excalidraw", &drawing(&[element("r", 1, "a0", 0)]));
            phone.write("Sketch.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
            phone.write("Sketch 2.excalidraw", &drawing(&[element("m", 1, "a0", 0)]));
            assert_eq!(phone.sync(&cloud), ["Sketch", "Sketch 3"]);
            assert_eq!(
                phone.files(),
                [
                    "Sketch 2.excalidraw",
                    "Sketch 3.excalidraw",
                    "Sketch.excalidraw"
                ]
            );
            assert_eq!(ids(&phone.read("Sketch.excalidraw").unwrap()), ["r"]);
            assert_eq!(ids(&phone.read("Sketch 3.excalidraw").unwrap()), ["l"]);
            assert_eq!(ids(&cloud.read("Sketch.excalidraw").unwrap()), ["r"]);
            assert_eq!(ids(&cloud.read("Sketch 3.excalidraw").unwrap()), ["l"]);
            assert_eq!(phone.sync(&cloud), Vec::<String>::new());
        }

        #[test]
        fn the_next_free_name_also_avoids_names_only_the_cloud_has() {
            let cloud = Cloud::new();
            let phone = Device::new();
            cloud.write("Sketch.excalidraw", &drawing(&[element("r", 1, "a0", 0)]));
            cloud.write("Sketch 2.excalidraw", &drawing(&[element("s", 1, "a0", 0)]));
            phone.write("Sketch.excalidraw", &drawing(&[element("l", 1, "a0", 0)]));
            phone.sync(&cloud);
            assert_eq!(ids(&phone.read("Sketch 3.excalidraw").unwrap()), ["l"]);
            assert_eq!(ids(&phone.read("Sketch 2.excalidraw").unwrap()), ["s"]);
        }

        #[test]
        fn two_devices_creating_the_same_name_offline_follow_the_same_rule() {
            let cloud = Cloud::new();
            let (phone, tablet, mac) = (Device::new(), Device::new(), Device::new());
            for (device, id) in [(&phone, "p"), (&tablet, "t")] {
                device.write("Untitled.excalidraw", &drawing(&[element(id, 1, "a0", 0)]));
            }
            mac.write("Untitled.excalidraw", &drawing(&[]));
            phone.sync(&cloud);
            tablet.sync(&cloud);
            assert_eq!(ids(&tablet.read("Untitled.excalidraw").unwrap()), ["p"]);
            assert_eq!(ids(&tablet.read("Untitled 2.excalidraw").unwrap()), ["t"]);
            phone.sync(&cloud);
            assert_eq!(phone.files(), tablet.files());

            // Empty on one side and drawn on the other are unrelated; no pointless copy
            // when both are empty.
            let (a, b) = (Device::new(), Device::new());
            let cloud = Cloud::new();
            a.write("Blank.excalidraw", &drawing(&[]));
            b.write("Blank.excalidraw", &drawing(&[]));
            a.sync(&cloud);
            b.sync(&cloud);
            assert_eq!(b.files(), ["Blank.excalidraw"]);
            mac.sync(&cloud);
        }

        #[test]
        fn connecting_again_merges_without_duplicates_also_after_both_sides_changed() {
            let (cloud, phone, tablet) = two_devices_apart();
            SyncState::forget(&phone.state.0).unwrap();
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
            );
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 2, "a1", 9)]),
            );
            tablet.sync(&cloud);
            phone.sync(&cloud);
            tablet.sync(&cloud);
            for side in [
                phone.read("Sketch.excalidraw"),
                tablet.read("Sketch.excalidraw"),
            ] {
                assert_eq!(
                    elements_of(&side.unwrap()),
                    [e("a", 2, false), e("b", 2, false)]
                );
            }
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
            assert_eq!(tablet.files(), ["Sketch.excalidraw"]);
        }

        #[test]
        fn switching_cloud_provider_merges_without_duplicates() {
            let (old_cloud, phone, _tablet) = two_devices_apart();
            // A second directory storage with the same Drawing, uploaded by another Device.
            let new_cloud = Cloud::new();
            let other = Device::new();
            other.sync(&old_cloud);
            // Connecting to another provider starts without history, as after a Disconnect.
            SyncState::forget(&other.state.0).unwrap();
            other.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 4, "a1", 0)]),
            );
            other.sync(&new_cloud);

            SyncState::forget(&phone.state.0).unwrap();
            phone.sync(&new_cloud);
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
            assert_eq!(
                elements_of(&phone.read("Sketch.excalidraw").unwrap()),
                [e("a", 1, false), e("b", 4, false)]
            );
            assert!(new_cloud.read("Sketch 2.excalidraw").is_none());
        }

        #[test]
        fn a_drawing_deleted_on_one_side_and_changed_on_the_other_is_deleted_everywhere() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.remove("Sketch.excalidraw");
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 3), element("b", 1, "a1", 0)]),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.sync(&cloud);
            assert!(cloud.read("Sketch.excalidraw").is_none());
            assert!(phone.files().is_empty() && tablet.files().is_empty());
        }

        #[test]
        fn a_drawing_deleted_on_one_side_and_unchanged_on_the_other_is_deleted_there() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.remove("Sketch.excalidraw");
            phone.sync(&cloud);
            assert!(cloud.read("Sketch.excalidraw").is_none());
            assert_eq!(tablet.sync(&cloud), ["Sketch"]);
            assert!(tablet.files().is_empty());
        }

        #[test]
        fn a_rename_crossing_an_edit_leaves_only_the_renamed_drawing_everywhere() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.rename("Sketch.excalidraw", "Renamed.excalidraw");
            tablet.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 3), element("b", 1, "a1", 0)]),
            );
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.sync(&cloud);
            for device in [&phone, &tablet] {
                assert_eq!(device.files(), ["Renamed.excalidraw"]);
            }
            assert_eq!(cloud.files(), ["Renamed.excalidraw"]);
        }

        #[test]
        fn a_rename_crossing_a_rename_leaves_both_new_names() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.rename("Sketch.excalidraw", "Phone.excalidraw");
            tablet.rename("Sketch.excalidraw", "Tablet.excalidraw");
            for device in [&phone, &tablet, &phone] {
                device.sync(&cloud);
            }
            for device in [&phone, &tablet] {
                assert_eq!(device.files(), ["Phone.excalidraw", "Tablet.excalidraw"]);
            }
        }

        #[test]
        fn a_delete_crossing_a_delete_is_a_delete() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.remove("Sketch.excalidraw");
            tablet.remove("Sketch.excalidraw");
            phone.sync(&cloud);
            assert!(tablet.sync(&cloud).is_empty());
            assert!(phone.sync(&cloud).is_empty());
            assert!(cloud.read("Sketch.excalidraw").is_none());
            assert!(phone.files().is_empty() && tablet.files().is_empty());
        }

        #[test]
        fn a_delete_reaches_three_devices_whatever_they_did_meanwhile() {
            let (cloud, phone, tablet) = two_devices_apart();
            let laptop = Device::new();
            laptop.sync(&cloud);
            phone.remove("Sketch.excalidraw");
            tablet.write("Sketch.excalidraw", b"edited on the tablet");
            laptop.rename("Sketch.excalidraw", "Laptop.excalidraw");
            for device in [&phone, &tablet, &laptop, &phone, &tablet, &laptop] {
                device.sync(&cloud);
            }
            for device in [&phone, &tablet, &laptop] {
                assert_eq!(device.files(), ["Laptop.excalidraw"]);
            }
        }

        #[test]
        fn a_delete_made_while_offline_wins_once_back_online() {
            // Offline is no Disconnect: the history stays, and the delete wins.
            let (cloud, phone, tablet) = two_devices_apart();
            phone.remove("Sketch.excalidraw");
            tablet.write("Sketch.excalidraw", b"edited while offline");
            phone.sync(&cloud);
            tablet.sync(&cloud);
            assert!(tablet.files().is_empty());
        }

        #[test]
        fn a_delete_in_a_step_interrupted_midway_is_finished_by_the_next_step() {
            let (cloud, phone, tablet) = two_devices_apart();
            phone.write("Other.excalidraw", b"other");
            phone.sync(&cloud);
            tablet.sync(&cloud);
            phone.remove("Sketch.excalidraw");
            phone.remove("Other.excalidraw");
            tablet.write("Sketch.excalidraw", b"edited");
            // The phone's step is cancelled before it does anything.
            let state = SyncState::open(&phone.state.0).unwrap();
            let outcome = sync_step(
                &phone.drawings.0,
                &state,
                &cloud.storage,
                &AtomicBool::new(true),
            );
            assert!(outcome.error.is_some());
            assert!(cloud.read("Sketch.excalidraw").is_some());
            for device in [&phone, &tablet, &phone] {
                device.sync(&cloud);
            }
            assert!(cloud.files().is_empty());
            assert!(phone.files().is_empty() && tablet.files().is_empty());
        }

        #[test]
        fn a_file_created_independently_under_the_name_of_a_deleted_one_is_uploaded() {
            let (cloud, phone, tablet) = two_devices_apart();
            let laptop = Device::new();
            phone.remove("Sketch.excalidraw");
            phone.sync(&cloud);
            // Never Synced on the laptop, so it has no history to delete by.
            laptop.write("Sketch.excalidraw", &drawing(&[element("z", 1, "a0", 0)]));
            laptop.sync(&cloud);
            assert!(cloud.read("Sketch.excalidraw").is_some());
            // The tablet, with the old one unchanged, and the phone, with none, take the new one.
            tablet.sync(&cloud);
            phone.sync(&cloud);
            let new = cloud.read("Sketch.excalidraw").unwrap();
            for device in [&phone, &tablet] {
                assert_eq!(device.read("Sketch.excalidraw").unwrap(), new);
            }
        }

        #[test]
        fn never_synced_files_are_never_deleted_around_connect_and_disconnect() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Sketch.excalidraw", b"one");
            phone.sync(&cloud);
            // First Connect of a Device whose file the cloud lacks (or no longer has).
            tablet.write("Mine.excalidraw", b"mine");
            tablet.sync(&cloud);
            assert_eq!(cloud.read("Mine.excalidraw").unwrap(), b"mine");
            assert_eq!(tablet.files(), ["Mine.excalidraw", "Sketch.excalidraw"]);

            // The cloud loses everything; after Disconnect plus Connect the Device's
            // files are uploaded again, not deleted.
            cloud.remove("Mine.excalidraw");
            cloud.remove("Sketch.excalidraw");
            SyncState::forget(&tablet.state.0).unwrap();
            tablet.sync(&cloud);
            assert_eq!(tablet.files(), ["Mine.excalidraw", "Sketch.excalidraw"]);
            assert_eq!(cloud.files(), ["Mine.excalidraw", "Sketch.excalidraw"]);

            // A file made after the last Sync is never touched by a cloud that lacks it.
            cloud.remove("Mine.excalidraw");
            tablet.write("New.excalidraw", b"new");
            tablet.sync(&cloud);
            assert_eq!(tablet.read("New.excalidraw").unwrap(), b"new");
            assert!(cloud.read("New.excalidraw").is_some());
        }

        #[test]
        fn a_drawing_deleted_while_disconnected_comes_back_on_the_next_connect() {
            let (cloud, phone, _tablet) = two_devices_apart();
            SyncState::forget(&phone.state.0).unwrap();
            phone.remove("Sketch.excalidraw");
            assert_eq!(phone.sync(&cloud), ["Sketch"]);
            assert_eq!(phone.files(), ["Sketch.excalidraw"]);
        }

        #[test]
        fn an_icloud_placeholder_is_absent_until_downloaded_and_a_later_step_finds_the_file() {
            let cloud_dir = Scratch::new("icloud");
            let asked = std::sync::Arc::new(Mutex::new(Vec::new()));
            let log = std::sync::Arc::clone(&asked);
            let storage = DirStorage::open_downloading(&cloud_dir.0, move |path| {
                log.lock().unwrap().push(path.to_path_buf());
            })
            .unwrap();
            let device = Device::new();
            let step = |device: &Device| {
                let state = SyncState::open(&device.state.0).unwrap();
                let outcome = sync_step(
                    &device.drawings.0,
                    &state,
                    &storage,
                    &AtomicBool::new(false),
                );
                assert!(outcome.error.is_none(), "{:?}", outcome.error);
                outcome.changed
            };
            fs::write(cloud_dir.0.join(".Sketch.excalidraw.icloud"), b"stub").unwrap();

            assert!(step(&device).is_empty());
            assert!(device.files().is_empty(), "not copied, not deleted");
            assert!(
                asked
                    .lock()
                    .unwrap()
                    .contains(&cloud_dir.0.join("Sketch.excalidraw"))
            );

            // The system has downloaded it.
            fs::remove_file(cloud_dir.0.join(".Sketch.excalidraw.icloud")).unwrap();
            fs::write(cloud_dir.0.join("Sketch.excalidraw"), b"one").unwrap();

            assert_eq!(step(&device), ["Sketch"]);
            assert_eq!(device.read("Sketch.excalidraw").unwrap(), b"one");
        }

        // --- Characterization -------------------------------------------------------

        /// A realistic set of Drawings (an image, renames, concurrent edits, an erase,
        /// a name clash) Synced by three Devices. What the Drawings folders and the
        /// cloud hold at the end was recorded from the engine before it learned about
        /// Context folders (`expected_final`), so that rewriting it can't change it.
        fn realistic_scenario(sync: impl Fn(&Device)) -> (Device, Device, Device) {
            let image = r#"{"f1":{"id":"f1","mimeType":"image/png","dataURL":"data:image/png;base64,iVBORw0KGgo="}}"#;
            let image_element = |id: &str, version, index: &str| {
                element(id, version, index, 0).replace("rectangle", "image")
            };
            let (phone, tablet, laptop) = (Device::new(), Device::new(), Device::new());
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 1, "a0", 0), element("b", 1, "a1", 0)]),
            );
            phone.write(
                "Map.excalidraw",
                &with(&[image_element("m", 1, "a0")], "{}", image),
            );
            phone.write("Plan.excalidraw", &drawing(&[element("p", 1, "a0", 0)]));
            for device in [&phone, &tablet, &laptop] {
                sync(device);
            }

            // Everyone changes something while apart.
            phone.write(
                "Sketch.excalidraw",
                &drawing(&[element("a", 2, "a0", 10), element("b", 1, "a1", 0)]),
            );
            phone.rename("Map.excalidraw", "Atlas.excalidraw");
            tablet.write(
        "Sketch.excalidraw",
        &with(
            &[
                element("a", 1, "a0", 0),
                element("b", 3, "a1", 30),
                image_element("n", 1, "a2"),
            ],
            "{}",
            r#"{"f2":{"id":"f2","mimeType":"image/png","dataURL":"data:image/png;base64,AAAA"}}"#,
        ),
    );
            laptop.write(
                "Sketch.excalidraw",
                &drawing(&[erased("a", 3, "a0"), element("b", 1, "a1", 0)]),
            );
            laptop.rename("Plan.excalidraw", "Plan B.excalidraw");
            laptop.write("Idea.excalidraw", &drawing(&[element("i", 1, "a0", 1)]));
            tablet.write("Idea.excalidraw", &drawing(&[element("j", 1, "a0", 2)]));

            for _ in 0..3 {
                for device in [&phone, &tablet, &laptop] {
                    sync(device);
                }
            }
            (phone, tablet, laptop)
        }

        /// Every file of a folder with its content, in name order.
        fn snapshot(dir: &Path) -> Vec<(String, String)> {
            listing(dir)
                .into_iter()
                .filter(|name| !name.starts_with('.'))
                .map(|name| {
                    let bytes = fs::read(dir.join(&name)).unwrap();
                    (name, String::from_utf8(bytes).unwrap())
                })
                .collect()
        }

        #[test]
        fn three_devices_with_a_realistic_set_of_drawings_end_with_the_recorded_files() {
            let cloud = Cloud::new();
            let (phone, tablet, laptop) = realistic_scenario(|device| {
                device.sync(&cloud);
            });

            let expected = expected_final();
            assert_eq!(snapshot(&cloud.dir.0), expected);
            for device in [&phone, &tablet, &laptop] {
                assert_eq!(snapshot(&device.drawings.0), expected);
            }
        }

        /// The files recorded from the engine before Context folders, by name, in
        /// `fixtures/realistic-drawings.json`.
        fn expected_final() -> Vec<(String, String)> {
            let path = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/realistic-drawings.json");
            let recorded = json::parse(&fs::read_to_string(path).unwrap()).unwrap();
            let mut files: Vec<_> = recorded
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, bytes)| (name.to_owned(), bytes.as_str().unwrap().to_owned()))
                .collect();
            files.sort();
            files
        }

        // --- Several Context folders ------------------------------------------------

        /// Files that are kept as they are, never merged: a Context without a merge.
        struct Opaque;

        impl Merge for Opaque {
            fn extension(&self) -> &str {
                ""
            }
            fn merge(&self, _: Option<&[u8]>, _: &[u8], _: &[u8]) -> Option<Vec<u8>> {
                None
            }
            fn is_item(&self, _: &[u8]) -> bool {
                false
            }
            fn same_item(&self, _: &[u8], _: &[u8]) -> Option<bool> {
                None
            }
        }

        /// A Device's second Context folder, "Notes", with its Sync state.
        struct Notes {
            dir: Scratch,
            state: Scratch,
        }

        impl Notes {
            fn new() -> Self {
                Notes {
                    dir: Scratch::new("notes"),
                    state: Scratch::new("notes-state"),
                }
            }

            fn write(&self, file: &str, bytes: &[u8]) {
                fs::write(self.dir.0.join(file), bytes).unwrap();
            }

            fn read(&self, file: &str) -> Option<Vec<u8>> {
                fs::read(self.dir.0.join(file)).ok()
            }
        }

        /// One step over the Drawings and the Notes folder of a Device, each with its
        /// own cloud folder. Returns the progress reported.
        fn sync_both(
            device: &Device,
            drawings_cloud: &Cloud,
            notes_cloud: &Cloud,
        ) -> Vec<(u32, u32)> {
            let drawings_state = SyncState::open(&device.state.0).unwrap();
            let notes_state = SyncState::open(&device.notes.state.0).unwrap();
            let folders = [
                Folder {
                    dir: &device.drawings.0,
                    state: &drawings_state,
                    storage: &drawings_cloud.storage,
                    merge: Arc::new(Drawings),
                },
                Folder {
                    dir: &device.notes.dir.0,
                    state: &notes_state,
                    storage: &notes_cloud.storage,
                    merge: Arc::new(Opaque),
                },
            ];
            let progress = Mutex::new(Vec::new());
            let outcomes = sync_folders(
                &folders,
                &AtomicBool::new(false),
                &|Progress { done, total }| progress.lock().unwrap().push((done, total)),
            );
            assert_eq!(outcomes.len(), 2);
            for outcome in &outcomes {
                assert!(outcome.error.is_none(), "{:?}", outcome.error);
            }
            progress.into_inner().unwrap()
        }

        #[test]
        fn the_realistic_scenario_gives_the_same_drawings_with_a_second_folder_beside_them() {
            let (cloud, notes_cloud) = (Cloud::new(), Cloud::new());
            let turn = AtomicU32::new(0);
            let (phone, tablet, laptop) = realistic_scenario(|device| {
                // A Notes file turns up on the Device before each Sync.
                let n = turn.fetch_add(1, Relaxed);
                device.notes.write(&format!("note-{n}.txt"), b"a note");
                sync_both(device, &cloud, &notes_cloud);
            });

            for device in [&phone, &tablet, &laptop] {
                sync_both(device, &cloud, &notes_cloud);
            }
            let expected = expected_final();
            assert_eq!(snapshot(&cloud.dir.0), expected);
            for device in [&phone, &tablet, &laptop] {
                assert_eq!(snapshot(&device.drawings.0), expected);
                // Every Notes file reached every Device, and no Drawing is among them.
                assert_eq!(
                    listing(&device.notes.dir.0).len(),
                    turn.load(Relaxed) as usize
                );
                assert!(
                    listing(&device.notes.dir.0)
                        .iter()
                        .all(|f| f.ends_with(".txt"))
                );
            }
            // Folders never leak into each other, on any side.
            assert!(
                cloud
                    .files()
                    .iter()
                    .all(|f| f.ends_with(".excalidraw") || f.starts_with('.'))
            );
            assert!(
                listing(&notes_cloud.dir.0)
                    .iter()
                    .all(|f| f.ends_with(".txt") || f.starts_with('.'))
            );
        }

        #[test]
        fn the_files_of_one_folder_never_reach_another_even_under_the_same_name() {
            let (cloud, notes_cloud) = (Cloud::new(), Cloud::new());
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write("Same.excalidraw", b"a drawing");
            phone.notes.write("Same.excalidraw", b"a note");

            sync_both(&phone, &cloud, &notes_cloud);
            sync_both(&tablet, &cloud, &notes_cloud);

            assert_eq!(tablet.read("Same.excalidraw").unwrap(), b"a drawing");
            assert_eq!(tablet.notes.read("Same.excalidraw").unwrap(), b"a note");
            assert_eq!(cloud.read("Same.excalidraw").unwrap(), b"a drawing");
            assert_eq!(notes_cloud.read("Same.excalidraw").unwrap(), b"a note");
        }

        #[test]
        fn an_opaque_folder_syncs_adds_changes_and_deletes_but_leaves_conflicts_alone() {
            let (cloud, notes_cloud) = (Cloud::new(), Cloud::new());
            let (phone, tablet) = (Device::new(), Device::new());
            phone.notes.write("a.txt", b"one");
            phone.notes.write("b.txt", b"one");
            sync_both(&phone, &cloud, &notes_cloud);
            sync_both(&tablet, &cloud, &notes_cloud);
            assert_eq!(tablet.notes.read("a.txt").unwrap(), b"one");

            // Changed on both sides: nothing is merged, both keep theirs.
            phone.notes.write("a.txt", b"phone");
            tablet.notes.write("a.txt", b"tablet");
            // Changed on one side only, and deleted on one side only.
            phone.notes.write("b.txt", b"two");
            phone.notes.write("c.txt", b"new");
            sync_both(&phone, &cloud, &notes_cloud);
            sync_both(&tablet, &cloud, &notes_cloud);
            sync_both(&phone, &cloud, &notes_cloud);

            assert_eq!(phone.notes.read("a.txt").unwrap(), b"phone");
            assert_eq!(tablet.notes.read("a.txt").unwrap(), b"tablet");
            assert_eq!(tablet.notes.read("b.txt").unwrap(), b"two");
            assert_eq!(tablet.notes.read("c.txt").unwrap(), b"new");
            fs::remove_file(tablet.notes.dir.0.join("c.txt")).unwrap();
            sync_both(&tablet, &cloud, &notes_cloud);
            sync_both(&phone, &cloud, &notes_cloud);
            assert_eq!(phone.notes.read("c.txt"), None);
        }

        #[test]
        fn an_idle_poll_is_one_listing_per_folder_and_nothing_else() {
            let (cloud, notes_cloud) = (Cloud::new(), Cloud::new());
            let phone = Device::new();
            phone.write("Sketch.excalidraw", b"one");
            phone.settle("Sketch.excalidraw");
            phone.notes.write("a.txt", b"one");
            let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
            fs::File::options()
                .write(true)
                .open(phone.notes.dir.0.join("a.txt"))
                .unwrap()
                .set_modified(old)
                .unwrap();
            // Uploads, then settles the stamps of both folders.
            for _ in 0..3 {
                sync_both(&phone, &cloud, &notes_cloud);
            }
            cloud.take_calls();
            notes_cloud.take_calls();

            sync_both(&phone, &cloud, &notes_cloud);

            for cloud in [&cloud, &notes_cloud] {
                let calls = cloud.take_calls();
                assert_eq!(calls.lists, 1);
                assert!(calls.gets.is_empty() && calls.puts.is_empty() && calls.deletes.is_empty());
            }
        }

        #[test]
        fn progress_counts_the_transfers_of_all_folders_together() {
            let (cloud, notes_cloud) = (Cloud::new(), Cloud::new());
            let phone = Device::new();
            phone.write("A.excalidraw", b"a");
            phone.write("B.excalidraw", b"b");
            phone.notes.write("c.txt", b"c");

            let progress = sync_both(&phone, &cloud, &notes_cloud);

            assert_eq!(progress, [(0, 2), (1, 2), (2, 2), (2, 3), (3, 3)]);
        }

        #[test]
        fn a_folder_that_fails_does_not_stop_the_others_from_reporting() {
            let cloud = Cloud::new();
            let phone = Device::new();
            phone.write("A.excalidraw", b"a");
            let state = SyncState::open(&phone.state.0).unwrap();
            let missing = Scratch::new("gone").0.join("not-here");
            let notes_state = SyncState::open(&phone.notes.state.0).unwrap();
            let notes_cloud = Cloud::new();
            let folders = [
                Folder {
                    dir: &missing,
                    state: &notes_state,
                    storage: &notes_cloud.storage,
                    merge: Arc::new(Opaque),
                },
                Folder {
                    dir: &phone.drawings.0,
                    state: &state,
                    storage: &cloud.storage,
                    merge: Arc::new(Drawings),
                },
            ];

            let outcomes = sync_folders(&folders, &AtomicBool::new(false), &|_| {});

            assert!(outcomes[0].error.is_some());
            assert!(outcomes[1].error.is_none());
            assert_eq!(cloud.read("A.excalidraw").unwrap(), b"a");
        }

        // ---- Books (ADR-0020, the Books merge hook) ----

        const BOOK_ID: &str = "aaaaaaaaaaaa";
        const OTHER_ID: &str = "bbbbbbbbbbbb";

        fn state_with(
            id: &str,
            name: &str,
            sha256: &str,
            highlights: &str,
            progress: &str,
            changes: &str,
        ) -> Vec<u8> {
            format!(
        r#"{{"id":"{id}","file_name":"{name}","sha256":"{sha256}","imported":1,"markup":{{"highlights":[{highlights}],"page_comments":[]}},"progress":{progress}{changes}}}"#
    )
    .into_bytes()
        }

        fn json(bytes: &[u8]) -> pkm_base::json::Value {
            pkm_base::json::parse(std::str::from_utf8(bytes).unwrap()).unwrap()
        }

        fn highlight_json(id: &str, color: u64, comment: Option<&str>) -> String {
            let comment = comment.map_or_else(String::new, |c| format!(r#","comment":"{c}""#));
            format!(
                r#"{{"id":"{id}","anchor":{{"epub":{{"start_spine":0,"start_offset":1,"end_spine":0,"end_offset":2}}}},"quoted_text":"text","color":{color},"created":1{comment}}}"#
            )
        }

        #[test]
        fn book_files_transfer_unchanged_and_a_name_clash_keeps_both() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Emma.epub", b"PK\x03\x04phone");
            tablet.write_book("Emma.epub", b"PK\x03\x04tablet");

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            assert_eq!(cloud.read("Emma.epub").unwrap(), b"PK\x03\x04phone");
            assert_eq!(cloud.read("Emma 2.epub").unwrap(), b"PK\x03\x04tablet");
            assert_eq!(tablet.book_files(), ["Emma 2.epub", "Emma.epub"]);
            assert_eq!(
                tablet.read_book("Emma 2.epub").unwrap(),
                b"PK\x03\x04tablet"
            );
            assert_eq!(phone.read_book("Emma.epub").unwrap(), b"PK\x03\x04phone");
        }

        #[test]
        fn a_book_file_changed_after_a_sync_reaches_the_other_devices() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);

            phone.write_book("Book.pdf", b"%PDF-two");
            phone.sync_books(&cloud);
            assert_eq!(cloud.read("Book.pdf").unwrap(), b"%PDF-two");

            tablet.sync_books(&cloud);
            assert_eq!(tablet.read_book("Book.pdf").unwrap(), b"%PDF-two");
        }

        #[test]
        fn book_files_changed_on_both_sides_are_both_kept_under_different_names() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);

            phone.write_book("Book.pdf", b"%PDF-phone");
            tablet.write_book("Book.pdf", b"%PDF-tablet");
            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            for device in [&phone, &tablet] {
                assert_eq!(device.book_files(), ["Book 2.pdf", "Book.pdf"]);
                assert_eq!(device.read_book("Book.pdf").unwrap(), b"%PDF-phone");
                assert_eq!(device.read_book("Book 2.pdf").unwrap(), b"%PDF-tablet");
            }
            assert_eq!(cloud.read("Book.pdf").unwrap(), b"%PDF-phone");
            assert_eq!(cloud.read("Book 2.pdf").unwrap(), b"%PDF-tablet");
        }

        #[test]
        fn two_books_that_shared_a_name_stay_two_books_with_their_own_markup() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Same.pdf", b"%PDF-phone");
            tablet.write_book("Same.pdf", b"%PDF-tablet");
            phone.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(
                    BOOK_ID,
                    "Same.pdf",
                    &sha(b"%PDF-phone"),
                    &highlight_json("phone", 1, None),
                    "null",
                    "",
                ),
            );
            tablet.write_book_state(
                &format!("{OTHER_ID}.json"),
                &state_with(
                    OTHER_ID,
                    "Same.pdf",
                    &sha(b"%PDF-tablet"),
                    &highlight_json("tablet", 1, None),
                    "null",
                    "",
                ),
            );

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            for device in [&phone, &tablet] {
                let library = Library::open(&device.books.0).unwrap();
                assert_eq!(library.books().unwrap().len(), 2);
                for (id, highlight) in [(BOOK_ID, "phone"), (OTHER_ID, "tablet")] {
                    let book = library.book(id).unwrap().unwrap();
                    assert_eq!(
                        book.sha256,
                        sha(if id == BOOK_ID {
                            b"%PDF-phone"
                        } else {
                            b"%PDF-tablet"
                        })
                    );
                    let markup = library.markup(id).unwrap();
                    assert_eq!(markup.highlights[0].id, highlight);
                }
            }
        }

        #[test]
        fn highlights_and_comments_merge_by_id_with_the_later_change_winning() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            let sha = "aa";
            phone.write_book("Book.pdf", b"%PDF-one");
            tablet.write_book("Book.pdf", b"%PDF-one");
            let left = state_with(
                BOOK_ID,
                "Book.pdf",
                sha,
                &highlight_json("h", 1, Some("old")),
                "null",
                r#","changes":{"highlight_colors":{"h":10},"highlight_comments":{"h":10}}"#,
            );
            let right = state_with(
                BOOK_ID,
                "Book.pdf",
                sha,
                &highlight_json("h", 2, Some("new")),
                "null",
                r#","changes":{"highlight_colors":{"h":11},"highlight_comments":{"h":11}}"#,
            );
            phone.write_book_state(&format!("{BOOK_ID}.json"), &left);
            tablet.write_book_state(&format!("{BOOK_ID}.json"), &right);

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            assert_eq!(
                phone.read_book_state(&format!("{BOOK_ID}.json")),
                tablet.read_book_state(&format!("{BOOK_ID}.json"))
            );
            let merged = json(&cloud.read(&format!("{BOOK_ID}.json")).unwrap());
            let highlight = &merged
                .get("markup")
                .unwrap()
                .get("highlights")
                .unwrap()
                .as_array()
                .unwrap()[0];
            assert_eq!(highlight.get("color").unwrap().as_u64(), Some(2));
            assert_eq!(highlight.get("comment").unwrap().as_str(), Some("new"));
        }

        #[test]
        fn reading_progress_goes_to_the_newest_last_read() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            tablet.write_book("Book.pdf", b"%PDF-one");
            phone.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(BOOK_ID, "Book.pdf", "aa", "", r#""e 1 5 100""#, ""),
            );
            tablet.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(BOOK_ID, "Book.pdf", "aa", "", r#"null"#, ""),
            );
            // The tablet read later.
            tablet.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(BOOK_ID, "Book.pdf", "aa", "", r#""e 2 1 200""#, ""),
            );

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            let merged = json(&phone.read_book_state(&format!("{BOOK_ID}.json")).unwrap());
            assert_eq!(merged.get("progress").unwrap().as_str(), Some("e 2 1 200"));
        }

        #[test]
        fn custom_colors_are_united_and_a_removal_beats_a_concurrent_use() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            tablet.write_book("Book.pdf", b"%PDF-one");
            phone.write_book_state(
        "preferences.json",
        br#"{"custom_colors":{"0000000000001-a":32896,"0000000000002-b":9849600},"default_color":"Green","default_color_at":10,"removed_colors":{"0000000000001-a":9}}"#,
    );
            tablet.write_book_state(
        "preferences.json",
        br#"{"custom_colors":{"0000000000001-a":32896,"0000000000003-c":255},"default_color":"Blue","default_color_at":11}"#,
    );

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            let merged = json(&phone.read_book_state("preferences.json").unwrap());
            let colors = merged.get("custom_colors").unwrap().as_object().unwrap();
            assert!(colors.get("0000000000001-a").is_none());
            assert!(colors.get("0000000000002-b").is_some());
            assert!(colors.get("0000000000003-c").is_some());
            assert_eq!(merged.get("default_color").unwrap().as_str(), Some("Blue"));
        }

        #[test]
        fn double_imports_while_apart_become_one_book_with_the_markup_of_both() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            for device in [&phone, &tablet] {
                device.write_book("Emma.epub", b"PK\x03\x04same");
            }
            phone.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(
                    BOOK_ID,
                    "Emma.epub",
                    sha(b"PK\x03\x04same").as_str(),
                    &highlight_json("one", 1, None),
                    "null",
                    "",
                ),
            );
            tablet.write_book_state(
                &format!("{OTHER_ID}.json"),
                &state_with(
                    OTHER_ID,
                    "Emma.epub",
                    sha(b"PK\x03\x04same").as_str(),
                    &highlight_json("two", 2, None),
                    "null",
                    "",
                ),
            );

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            for device in [&phone, &tablet] {
                let library = Library::open(&device.books.0).unwrap();
                let books = library.books().unwrap();
                assert_eq!(books.len(), 1);
                let markup = library.markup(&books[0].id).unwrap();
                assert_eq!(markup.highlights.len(), 2);
            }
        }

        #[test]
        fn deleting_a_book_beats_a_highlight_made_meanwhile() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            phone.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(BOOK_ID, "Book.pdf", "aa", "", "null", ""),
            );
            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);

            phone.remove_book("Book.pdf");
            phone.remove_book_state(&format!("{BOOK_ID}.json"));
            tablet.write_book_state(
                &format!("{BOOK_ID}.json"),
                &state_with(
                    BOOK_ID,
                    "Book.pdf",
                    "aa",
                    &highlight_json("h", 1, None),
                    "null",
                    "",
                ),
            );
            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);

            assert_eq!(cloud.read("Book.pdf"), None);
            assert!(cloud.read(&format!("{BOOK_ID}.json")).is_none());
            assert!(tablet.read_book("Book.pdf").is_none());
            assert!(tablet.read_book_state(&format!("{BOOK_ID}.json")).is_none());
        }

        #[test]
        fn an_unparseable_state_file_is_left_alone() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_book("Book.pdf", b"%PDF-one");
            let good = state_with(BOOK_ID, "Book.pdf", "aa", "", "null", "");
            phone.write_book_state(&format!("{BOOK_ID}.json"), &good);
            let corrupt = b"{\"id\": ".to_vec();
            tablet.write_book_state(&format!("{BOOK_ID}.json"), &corrupt);

            phone.sync_books(&cloud);
            tablet.sync_books(&cloud);
            phone.sync_books(&cloud);

            assert_eq!(cloud.read(&format!("{BOOK_ID}.json")).unwrap(), good);
            assert_eq!(
                tablet.read_book_state(&format!("{BOOK_ID}.json")).unwrap(),
                corrupt
            );
        }

        fn sha(bytes: &[u8]) -> String {
            let mut hasher = pkm_base::sha256::Sha256::new();
            hasher.update(bytes);
            hasher.finalize_hex()
        }

        #[test]
        fn three_devices_converge_on_one_state_with_every_highlight_and_the_latest_fields() {
            let cloud = Cloud::new();
            let devices = [Device::new(), Device::new(), Device::new()];
            let marks = [("a", 10, "first"), ("b", 20, "second"), ("c", 30, "third")];
            for (device, (own, at, comment)) in devices.iter().zip(marks) {
                device.write_book("Book.pdf", b"%PDF-one");
                let highlights = format!(
                    "{},{}",
                    highlight_json("h", 1, Some(comment)),
                    highlight_json(own, 2, None)
                );
                let changes = format!(
                    r#","changes":{{"highlight_comments":{{"h":{at}}},"highlight_colors":{{"h":{at}}}}}"#
                );
                device.write_book_state(
                    &format!("{BOOK_ID}.json"),
                    &state_with(BOOK_ID, "Book.pdf", "aa", &highlights, "null", &changes),
                );
            }

            for _ in 0..3 {
                for device in &devices {
                    device.sync_books(&cloud);
                }
            }

            let merged = cloud.read(&format!("{BOOK_ID}.json")).unwrap();
            for device in &devices {
                assert_eq!(
                    device.read_book_state(&format!("{BOOK_ID}.json")).unwrap(),
                    merged
                );
            }
            let state = json(&merged);
            assert_eq!(
                state
                    .get("markup")
                    .unwrap()
                    .get("highlights")
                    .unwrap()
                    .as_array()
                    .unwrap()
                    .len(),
                4
            );
            let common = state
                .get("markup")
                .unwrap()
                .get("highlights")
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .find(|h| h.get("id").unwrap().as_str() == Some("h"))
                .unwrap();
            assert_eq!(common.get("comment").unwrap().as_str(), Some("third"));
        }

        // ---- Tasks (ADR-0020, ADR-0021, ADR-0022) ----

        fn task(title: &str, priority: u8, tags: &[&str], updated: &str) -> Vec<u8> {
            let tags: String = tags.iter().map(|t| format!("  - {t}\n")).collect();
            let tags = if tags.is_empty() {
                tags
            } else {
                format!("tags:\n{tags}")
            };
            format!(
        "---\nid: aaaaaaaaaaaa\ntitle: {title}\npriority: {priority}\nstate: active\norder: 1\n{tags}updated: {updated}\n---\n"
    )
    .into_bytes()
        }

        fn text(bytes: Vec<u8>) -> String {
            String::from_utf8(bytes).unwrap()
        }

        #[test]
        fn a_task_reaches_the_other_device() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            let file = task("Milk", 4, &[], "2024-01-01T00:00:00Z");
            phone.write_task("aaaaaaaaaaaa.md", &file);

            phone.sync_tasks(&cloud);
            let changed = tablet.sync_tasks(&cloud);

            assert_eq!(changed, ["aaaaaaaaaaaa"]);
            assert_eq!(tablet.read_task("aaaaaaaaaaaa.md").unwrap(), file);
        }

        #[test]
        fn edits_to_different_fields_on_two_devices_are_both_kept() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_task(
                "aaaaaaaaaaaa.md",
                &task("Milk", 4, &["home"], "2024-01-01T00:00:00Z"),
            );
            phone.sync_tasks(&cloud);
            tablet.sync_tasks(&cloud);

            phone.write_task(
                "aaaaaaaaaaaa.md",
                &task("Oat milk", 4, &["home", "shop"], "2024-01-02T00:00:00Z"),
            );
            tablet.write_task(
                "aaaaaaaaaaaa.md",
                &task("Milk", 1, &["home", "food"], "2024-01-03T00:00:00Z"),
            );
            phone.sync_tasks(&cloud);
            tablet.sync_tasks(&cloud);
            phone.sync_tasks(&cloud);

            let merged = text(phone.read_task("aaaaaaaaaaaa.md").unwrap());
            assert!(merged.contains("title: Oat milk\n"), "{merged}");
            assert!(merged.contains("priority: 1\n"), "{merged}");
            for tag in ["home", "shop", "food"] {
                assert!(merged.contains(&format!("  - {tag}\n")), "{merged}");
            }
            assert_eq!(
                tablet.read_task("aaaaaaaaaaaa.md"),
                phone.read_task("aaaaaaaaaaaa.md")
            );
            assert_eq!(
                cloud.read("aaaaaaaaaaaa.md"),
                phone.read_task("aaaaaaaaaaaa.md")
            );
        }

        #[test]
        fn a_task_file_removed_on_one_device_is_deleted_everywhere() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_task(
                "aaaaaaaaaaaa.md",
                &task("Milk", 4, &[], "2024-01-01T00:00:00Z"),
            );
            phone.sync_tasks(&cloud);
            tablet.sync_tasks(&cloud);

            phone.remove_task("aaaaaaaaaaaa.md");
            tablet.write_task(
                "aaaaaaaaaaaa.md",
                &task("Milk", 1, &[], "2024-01-02T00:00:00Z"),
            );
            phone.sync_tasks(&cloud);
            tablet.sync_tasks(&cloud);

            assert!(cloud.read("aaaaaaaaaaaa.md").is_none());
            assert!(tablet.read_task("aaaaaaaaaaaa.md").is_none());
        }

        #[test]
        fn a_file_that_is_no_task_is_neither_transferred_nor_overwritten() {
            let cloud = Cloud::new();
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_task("bbbbbbbbbbbb.md", b"not a task");
            phone.sync_tasks(&cloud);
            assert!(cloud.read("bbbbbbbbbbbb.md").is_none());

            cloud.write("cccccccccccc.md", b"---\nbroken");
            tablet.sync_tasks(&cloud);
            assert!(tablet.read_task("cccccccccccc.md").is_none());
        }

        #[test]
        fn tasks_sync_alongside_drawings_and_books() {
            let (drawings, books, tasks) = (Cloud::new(), Cloud::new(), Cloud::new());
            let (phone, tablet) = (Device::new(), Device::new());
            phone.write_task(
                "aaaaaaaaaaaa.md",
                &task("Milk", 4, &[], "2024-01-01T00:00:00Z"),
            );
            phone.write_book("Book.pdf", b"%PDF-one");
            phone.write("Sketch.excalidraw", b"{}");
            for device in [&phone, &tablet] {
                device.sync(&drawings);
                device.sync_books(&books);
                device.sync_tasks(&tasks);
            }

            assert!(tablet.read_task("aaaaaaaaaaaa.md").is_some());
            assert_eq!(tablet.read_book("Book.pdf").unwrap(), b"%PDF-one");
            assert_eq!(tablet.files(), ["Sketch.excalidraw"]);
        }

        /// A Device with a Tasks store, its Tags beside it as on a real Device.
        struct TaggingDevice {
            root: Scratch,
        }

        impl TaggingDevice {
            fn new() -> Self {
                Self {
                    root: Scratch::new("tagging"),
                }
            }

            /// The store, opened fresh as at a launch.
            fn store(&self) -> pkm_tasks::Tasks {
                pkm_tasks::Tasks::open(self.root.0.join("Tasks")).unwrap()
            }

            /// One Sync step of the Tags and of the Task files, each with its cloud.
            fn sync(&self, tags: &Cloud, tasks: &Cloud) {
                fs::create_dir_all(self.root.0.join("state")).unwrap();
                let state = |name| SyncState::open(self.root.0.join("state").join(name)).unwrap();
                let (tags_state, tasks_state) = (state("Tags"), state("Tasks"));
                let (tags_dir, tasks_dir) = (self.root.0.join("Tags"), self.root.0.join("Tasks"));
                fs::create_dir_all(&tags_dir).unwrap();
                fs::create_dir_all(&tasks_dir).unwrap();
                let folders = [
                    Folder {
                        dir: &tags_dir,
                        state: &tags_state,
                        storage: &tags.storage,
                        merge: Arc::new(TagsFile),
                    },
                    Folder {
                        dir: &tasks_dir,
                        state: &tasks_state,
                        storage: &tasks.storage,
                        merge: Arc::new(Tasks),
                    },
                ];
                for outcome in sync_folders(&folders, &AtomicBool::new(false), &|_| {}) {
                    assert!(outcome.error.is_none(), "{:?}", outcome.error);
                }
            }

            /// One Sync step of the Filters, with their cloud.
            fn sync_filters(&self, filters: &Cloud) {
                fs::create_dir_all(self.root.0.join("state")).unwrap();
                let state = SyncState::open(self.root.0.join("state").join("Filters")).unwrap();
                let dir = self.root.0.join("Filters");
                fs::create_dir_all(&dir).unwrap();
                let folders = [Folder {
                    dir: &dir,
                    state: &state,
                    storage: &filters.storage,
                    merge: Arc::new(Filters),
                }];
                for outcome in sync_folders(&folders, &AtomicBool::new(false), &|_| {}) {
                    assert!(outcome.error.is_none(), "{:?}", outcome.error);
                }
            }

            /// The Filters' Titles in order, as the store lists them.
            fn filter_titles(&self) -> Vec<String> {
                let store = self.store();
                store
                    .filters()
                    .unwrap()
                    .into_iter()
                    .map(|f| f.title)
                    .collect()
            }

            /// The Tags with their Tag Colors, as the store lists them.
            fn tags(&self) -> Vec<(String, Option<String>)> {
                let store = self.store();
                let _ = store.rows(pkm_tasks::Planning::Active); // reads what Sync brought
                store
                    .tags()
                    .list()
                    .into_iter()
                    .map(|e| (e.name, e.color))
                    .collect()
            }
        }

        fn tag(name: &str, color: Option<&str>) -> (String, Option<String>) {
            (name.to_owned(), color.map(str::to_owned))
        }

        #[test]
        fn tags_and_their_colors_sync_and_merge_between_devices() {
            let (tags, tasks) = (Cloud::new(), Cloud::new());
            let (phone, tablet) = (TaggingDevice::new(), TaggingDevice::new());
            let store = phone.store();
            let milk = store
                .create("Milk", pkm_tasks::Planning::Active, 4, &[])
                .unwrap();
            store.add_tag(&milk.id, "home").unwrap();
            store.add_tag(&milk.id, "work").unwrap();
            phone.sync(&tags, &tasks);
            tablet.sync(&tags, &tasks);
            assert_eq!(tablet.tags(), [tag("home", None), tag("work", None)]);

            // Both change the Tag Color of `work`, the tablet later; the phone adds a Tag.
            store.set_tag_color("work", Some("#111111")).unwrap();
            store.add_tag(&milk.id, "errand").unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
            tablet
                .store()
                .set_tag_color("work", Some("#222222"))
                .unwrap();
            phone.sync(&tags, &tasks);
            tablet.sync(&tags, &tasks);
            phone.sync(&tags, &tasks);

            let expected = [
                tag("errand", None),
                tag("home", None),
                tag("work", Some("#222222")),
            ];
            assert_eq!(tablet.tags(), expected);
            assert_eq!(phone.tags(), expected);
        }

        #[test]
        fn a_tag_put_on_a_task_while_another_device_deleted_it_comes_back_grey_on_that_task_only() {
            let (tags, tasks) = (Cloud::new(), Cloud::new());
            let (phone, tablet) = (TaggingDevice::new(), TaggingDevice::new());
            let store = phone.store();
            let planning = pkm_tasks::Planning::Active;
            let (milk, eggs) = (
                store.create("Milk", planning, 4, &[]).unwrap(),
                store.create("Eggs", planning, 4, &[]).unwrap(),
            );
            store.add_tag(&milk.id, "work").unwrap();
            store.set_tag_color("work", Some("#d0021b")).unwrap();
            phone.sync(&tags, &tasks);
            tablet.sync(&tags, &tasks);

            // The phone Deletes `work`; the tablet puts it on Eggs.
            store.remove_tag_everywhere("work").unwrap();
            store.forget_tag("work").unwrap();
            tablet.store().add_tag(&eggs.id, "work").unwrap();
            phone.sync(&tags, &tasks);
            tablet.sync(&tags, &tasks);
            phone.sync(&tags, &tasks);

            for device in [&phone, &tablet] {
                assert_eq!(device.tags(), [tag("work", None)]);
                let tagged: Vec<_> = device
                    .store()
                    .rows(planning)
                    .unwrap()
                    .into_iter()
                    .filter(|r| r.task.tags.contains(&"work".to_owned()))
                    .map(|r| r.task.title)
                    .collect();
                assert_eq!(tagged, ["Eggs"]);
            }
        }

        #[test]
        fn filters_sync_and_merge_between_devices_and_a_delete_wins() {
            let filters = Cloud::new();
            let (phone, tablet) = (TaggingDevice::new(), TaggingDevice::new());
            let store = phone.store();
            let work = store.create_filter("Work", "@work").unwrap();
            let home = store.create_filter("Home", "@home").unwrap();
            let gone = store.create_filter("Gone", "@gone").unwrap();
            phone.sync_filters(&filters);
            tablet.sync_filters(&filters);
            // The Favorites both Devices created are one set.
            let favorites = ["All Tasks", "Active", "Backlog"];
            let with = |titles: &[&str]| -> Vec<String> {
                favorites
                    .iter()
                    .chain(titles)
                    .map(|t| (*t).to_owned())
                    .collect()
            };
            assert_eq!(tablet.filter_titles(), with(&["Work", "Home", "Gone"]));

            // Different fields of one Filter, a Delete of another one.
            store.edit_filter(&work.id, "Job", "@work").unwrap();
            store.delete_filter(&gone.id).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(1100));
            let other = tablet.store();
            other.set_favorite(&work.id, true).unwrap();
            other
                .edit_filter(&gone.id, "Gone for good", "@gone")
                .unwrap();
            other
                .edit_filter(&home.id, "Home", "@home & !@wait")
                .unwrap();
            for device in [&phone, &tablet, &phone] {
                device.sync_filters(&filters);
            }

            for device in [&phone, &tablet] {
                assert_eq!(device.filter_titles(), with(&["Job", "Home"]));
                let listed = device.store().filters().unwrap();
                assert!(listed[3].favorite);
                assert_eq!(listed[4].query, "@home & !@wait");
            }
        }
    }
}
