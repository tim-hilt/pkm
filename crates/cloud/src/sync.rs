//! The Sync engine (ADR-0011): one step makes the Device's Drawings folder and
//! the cloud folder agree for every Drawing changed on one side only.
//!
//! A Drawing is its file name, `<name>.excalidraw`, on both sides. For each
//! name the step compares the local file and the cloud blob with what this
//! Device remembers of the last Sync (`SyncState`): the bytes then, and the
//! blob's change marker then. A Drawing changed on both sides, or present on
//! both without any history, is left as it is until the merge exists.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use crate::storage::{Storage, write_file_atomically};

/// The file extension of a Drawing, with its dot.
const EXTENSION: &str = ".excalidraw";

/// What this Device remembers of each Drawing at its last Sync, in a folder of
/// its own: never inside the Drawings folder, never Synced. Forgotten on
/// Disconnect (`forget`).
#[derive(Debug, Clone)]
pub struct SyncState {
    dir: PathBuf,
}

/// A Drawing as of the last Sync.
struct Synced {
    bytes: Vec<u8>,
    /// The cloud blob's change marker when it was last seen.
    marker: String,
}

impl SyncState {
    /// The state in `dir`, created if missing.
    pub fn open(dir: impl Into<PathBuf>) -> io::Result<Self> {
        let dir = dir.into();
        fs::create_dir_all(dir.join("bytes"))?;
        fs::create_dir_all(dir.join("markers"))?;
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

    /// Remembers `bytes` and `marker`. The marker goes last: without it the
    /// bytes count for nothing.
    fn set(&self, file: &str, bytes: &[u8], marker: &str) -> io::Result<()> {
        write_file_atomically(&self.dir.join("bytes").join(file), bytes)?;
        write_file_atomically(&self.dir.join("markers").join(file), marker.as_bytes())
    }

    fn remove(&self, file: &str) -> io::Result<()> {
        for kind in ["markers", "bytes"] {
            match fs::remove_file(self.dir.join(kind).join(file)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }

    fn files(&self) -> io::Result<Vec<String>> {
        file_names(&self.dir.join("markers"))
    }
}

/// What one Sync step did.
#[derive(Debug, Default)]
pub struct StepOutcome {
    /// The names of the Drawings whose local file the step wrote or deleted.
    /// A rename shows as the old name and the new one.
    pub changed: Vec<String>,
    /// The first thing that went wrong. The step carries on with the other
    /// Drawings, unless it was cancelled, so changes still arrive while e.g.
    /// uploads fail. Cancelling is `io::ErrorKind::Interrupted`.
    pub error: Option<io::Error>,
}

/// Runs one Sync step between `drawings` (the Device's Drawings folder) and
/// `storage`, and lists the cloud once when nothing changed. Stops as soon as
/// `cancel` is set, also in the middle of a transfer.
pub fn sync_step(
    drawings: &Path,
    state: &SyncState,
    storage: &dyn Storage,
    cancel: &AtomicBool,
) -> StepOutcome {
    let mut step = Step {
        drawings,
        state,
        storage,
        cancel,
        remote: HashMap::new(),
        uploaded: Vec::new(),
        outcome: StepOutcome::default(),
    };
    if let Err(error) = step.run() {
        let error = step.cancelled_or(error);
        step.outcome.error.get_or_insert(error);
    }
    step.outcome
}

struct Step<'a> {
    drawings: &'a Path,
    state: &'a SyncState,
    storage: &'a dyn Storage,
    cancel: &'a AtomicBool,
    /// File name → change marker of the Drawings in the cloud.
    remote: HashMap<String, String>,
    /// The Drawings put in the cloud, whose new markers are not known yet.
    uploaded: Vec<(String, Vec<u8>)>,
    outcome: StepOutcome,
}

impl Step<'_> {
    fn run(&mut self) -> io::Result<()> {
        self.remote = self.list_remote()?;
        let mut files: BTreeSet<String> = self.remote.keys().cloned().collect();
        files.extend(file_names(self.drawings)?);
        files.extend(self.state.files()?);

        for file in files {
            self.check_cancelled()?;
            if let Err(error) = self.sync_drawing(&file) {
                // Whatever the error, cancelling is why the transfer failed.
                self.check_cancelled()?;
                self.outcome.error.get_or_insert(error);
            }
        }
        self.remember_uploads()
    }

    fn list_remote(&self) -> io::Result<HashMap<String, String>> {
        Ok(self
            .storage
            .list()?
            .into_iter()
            .filter(|blob| is_drawing(&blob.name))
            .map(|blob| (blob.name, blob.marker))
            .collect())
    }

    /// `error`, or `Interrupted` if the step was cancelled meanwhile: readers
    /// can't fail with `Interrupted` themselves, since `std` retries those.
    fn cancelled_or(&self, error: io::Error) -> io::Error {
        match self.check_cancelled() {
            Err(cancelled) => cancelled,
            Ok(()) => error,
        }
    }

    fn check_cancelled(&self) -> io::Result<()> {
        if self.cancel.load(Relaxed) {
            Err(io::Error::from(io::ErrorKind::Interrupted))
        } else {
            Ok(())
        }
    }

    /// The rules of the spec for one name, with L the local file, R the cloud
    /// blob and B the last-Synced state.
    fn sync_drawing(&mut self, file: &str) -> io::Result<()> {
        let local = self.read_local(file)?;
        let remote = self.remote.get(file).cloned();
        let base = self.state.get(file)?;

        match (local, remote, base) {
            (None, None, _) => self.state.remove(file),
            (Some(local), None, None) => self.upload(file, local),
            (None, Some(marker), None) => self.download(file, &marker, None),
            // Never Synced under this name, present on both sides.
            (Some(_), Some(_), None) => Ok(()),
            (local, remote, Some(base)) => {
                let local_changed = local.as_deref() != Some(&base.bytes[..]);
                let remote_changed = remote.as_deref() != Some(&base.marker[..]);
                match (local, remote) {
                    (Some(local), Some(marker)) => match (local_changed, remote_changed) {
                        (true, false) => self.upload(file, local),
                        (false, true) => self.download(file, &marker, Some(&local)),
                        _ => Ok(()), // unchanged, or changed on both sides
                    },
                    // Deleted here: reaches the cloud, unless it was edited there.
                    (None, Some(marker)) if remote_changed => self.download(file, &marker, None),
                    (None, Some(_)) => {
                        self.storage.delete(file)?;
                        self.state.remove(file)
                    }
                    // Deleted there: reaches here, unless it was edited here.
                    (Some(local), None) if local_changed => self.upload(file, local),
                    (Some(local), None) => self.delete_local(file, &local),
                    (None, None) => Ok(()),
                }
            }
        }
    }

    fn path(&self, file: &str) -> PathBuf {
        self.drawings.join(file)
    }

    fn read_local(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        match fs::read(self.path(file)) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            // Not a file (a folder called so): not a Drawing.
            Err(e) if e.kind() == io::ErrorKind::IsADirectory => Ok(None),
            other => other.map(Some),
        }
    }

    fn upload(&mut self, file: &str, bytes: Vec<u8>) -> io::Result<()> {
        let mut data = Cancellable {
            inner: &bytes[..],
            cancel: self.cancel,
        };
        self.storage.put(file, &mut data)?;
        self.uploaded.push((file.to_owned(), bytes));
        Ok(())
    }

    /// Writes the cloud's Drawing here and remembers it. `expected` is the
    /// local file as it was read; if the app wrote it since, it is left for
    /// the next step.
    fn download(&mut self, file: &str, marker: &str, expected: Option<&[u8]>) -> io::Result<()> {
        let Some(data) = self.storage.get(file)? else {
            return Ok(()); // deleted since the listing
        };
        let mut bytes = Vec::new();
        Cancellable {
            inner: data,
            cancel: self.cancel,
        }
        .read_to_end(&mut bytes)?;
        if self.read_local(file)?.as_deref() != expected {
            return Ok(());
        }
        write_file_atomically(&self.path(file), &bytes)?;
        self.state.set(file, &bytes, marker)?;
        self.outcome.changed.push(name_of(file));
        Ok(())
    }

    fn delete_local(&mut self, file: &str, expected: &[u8]) -> io::Result<()> {
        if self.read_local(file)?.as_deref() == Some(expected) {
            fs::remove_file(self.path(file))?;
            self.outcome.changed.push(name_of(file));
        }
        self.state.remove(file)
    }

    /// Remembers the uploads with the markers the cloud gave them, which a
    /// listing tells. One listing for all of them. Without it they are
    /// uploaded again next time, which does no harm.
    fn remember_uploads(&mut self) -> io::Result<()> {
        if self.uploaded.is_empty() {
            return Ok(());
        }
        self.check_cancelled()?;
        let markers = self.list_remote()?;
        for (file, bytes) in std::mem::take(&mut self.uploaded) {
            if let Some(marker) = markers.get(&file) {
                self.state.set(&file, &bytes, marker)?;
            }
        }
        Ok(())
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

fn is_drawing(file: &str) -> bool {
    file.len() > EXTENSION.len() && file.ends_with(EXTENSION) && !file.starts_with('.')
}

fn name_of(file: &str) -> String {
    file.strip_suffix(EXTENSION).unwrap_or(file).to_owned()
}

/// The names of the Drawing files in `dir`.
fn file_names(dir: &Path) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') && entry.metadata()?.is_file() {
            names.push(name);
        }
    }
    Ok(names.into_iter().filter(|n| is_drawing(n)).collect())
}
