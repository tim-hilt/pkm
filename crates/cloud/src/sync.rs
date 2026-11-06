//! The Sync engine (ADR-0011): one step makes the Device's Drawings folder and
//! the cloud folder agree for every Drawing changed on one side only.
//!
//! A Drawing is its file name, `<name>.excalidraw`, on both sides. For each
//! name the step compares the local file and the cloud blob with what this
//! Device remembers of the last Sync (`SyncState`): the bytes then, and the
//! blob's change marker then.
//!
//! A Drawing present on both sides without any history is merged without a
//! base if both are the same Drawing (`are_same_drawing`); otherwise both are
//! kept and the local one is renamed to the next free name ("Sketch 2").
//!
//! A Drawing changed on both sides is merged (`merge_drawings`) and the result
//! written to both. The same merge runs when only the cloud changed, since the
//! change may be another Device's upload overwriting ours without having seen
//! it (there is no conditional write): what the cloud then lacks is uploaded
//! again. A Drawing that can't be parsed is skipped, and left as it is on both
//! sides.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};

use excalidraw_native_core::{are_same_drawing, merge_drawings};

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
    /// The cloud rejected the credentials. The step stopped.
    pub access_lost: bool,
    /// An upload found the cloud full (or over its quota). Uploads stopped for
    /// this step, downloads went on.
    pub storage_full: bool,
}

/// How far a step is through the Drawings it transfers. Deletes and the
/// exchange of state alone don't count: a step with nothing to transfer
/// reports no progress at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
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
    let mut step = Step {
        drawings,
        state,
        storage,
        cancel,
        progress,
        done: 0,
        total: 0,
        remote: HashMap::new(),
        uploaded: Vec::new(),
        outcome: StepOutcome::default(),
    };
    if let Err(error) = step.run() {
        let error = step.cancelled_or(error);
        step.record(error);
    }
    step.outcome
}

struct Step<'a> {
    drawings: &'a Path,
    state: &'a SyncState,
    storage: &'a dyn Storage,
    cancel: &'a AtomicBool,
    progress: &'a dyn Fn(Progress),
    /// Drawings to transfer in this step, and how many are done.
    total: u32,
    done: u32,
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

        for file in &files {
            self.check_cancelled()?;
            let (local, remote, base) = self.facts(file)?;
            if decide(local.as_deref(), remote.as_deref(), base.as_ref()).is_transfer() {
                self.total += 1;
            }
        }
        self.report();

        for file in files {
            self.check_cancelled()?;
            if let Err(error) = self.sync_drawing(&file) {
                // Whatever the error, cancelling is why the transfer failed.
                self.check_cancelled()?;
                self.record(error);
                if self.outcome.access_lost {
                    return Ok(()); // every further call would be rejected too
                }
            }
        }
        self.remember_uploads()
    }

    /// Notes what went wrong: lost access and a full cloud are problems of
    /// their own, anything else is an error (offline, most likely).
    fn record(&mut self, error: io::Error) {
        match error.kind() {
            io::ErrorKind::PermissionDenied => self.outcome.access_lost = true,
            io::ErrorKind::StorageFull => self.outcome.storage_full = true,
            _ => {
                self.outcome.error.get_or_insert(error);
            }
        }
    }

    fn report(&self) {
        if self.total > 0 {
            (self.progress)(Progress {
                done: self.done,
                total: self.total,
            });
        }
    }

    /// The local file, the cloud blob's marker and the last-Synced state.
    #[allow(clippy::type_complexity)]
    fn facts(&self, file: &str) -> io::Result<(Option<Vec<u8>>, Option<String>, Option<Synced>)> {
        Ok((
            self.read_local(file)?,
            self.remote.get(file).cloned(),
            self.state.get(file)?,
        ))
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

    /// The rules of the spec for one name (`decide`).
    fn sync_drawing(&mut self, file: &str) -> io::Result<()> {
        let (local, remote, base) = self.facts(file)?;
        let action = decide(local.as_deref(), remote.as_deref(), base.as_ref());
        let result = match (action, local, remote, base) {
            (Action::Forget, ..) => self.state.remove(file),
            (Action::Upload, Some(local), ..) => self.upload(file, local),
            (Action::Download, _, Some(marker), _) => self.download(file, &marker, None),
            (Action::FirstMeeting, Some(local), Some(marker), _) => {
                self.first_meeting(file, &marker, local)
            }
            (Action::Reconcile { local_changed }, Some(local), Some(marker), Some(base)) => {
                self.reconcile(file, &marker, local, &base, local_changed)
            }
            (Action::DeleteRemote, ..) => {
                self.storage.delete(file)?;
                self.state.remove(file)
            }
            (Action::DeleteLocal, Some(local), ..) => self.delete_local(file, &local),
            _ => Ok(()),
        };
        if action.is_transfer() {
            self.done += 1;
            self.report();
        }
        result
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

    /// The cloud's Drawing changed since the last Sync: merges it with the
    /// local one. What the merge gives goes to the side that lacks it.
    fn reconcile(
        &mut self,
        file: &str,
        marker: &str,
        local: Vec<u8>,
        base: &Synced,
        local_changed: bool,
    ) -> io::Result<()> {
        let Some(remote) = self.fetch(file)? else {
            return Ok(()); // deleted since the listing
        };
        let merged = match merge_drawings(Some(&base.bytes), &local, &remote) {
            Ok(merged) => merged,
            // Not a Drawing on one side: skipped when both changed, else the
            // cloud's file is copied as it is.
            Err(_) if local_changed => return Ok(()),
            Err(_) => return self.write_local(file, &remote, marker, Some(&local)),
        };
        if merged != local && !self.replace_local(file, &merged, Some(&local))? {
            return Ok(());
        }
        if merged == remote {
            self.state.set(file, &remote, marker)
        } else {
            self.upload(file, merged)
        }
    }

    /// A name both sides have, with no history between them. The same
    /// Drawing is merged without a base; unrelated ones are both kept, the
    /// local one under the next free name. A file that is no Drawing is
    /// skipped.
    fn first_meeting(&mut self, file: &str, marker: &str, local: Vec<u8>) -> io::Result<()> {
        let Some(remote) = self.fetch(file)? else {
            return Ok(()); // deleted since the listing
        };
        match are_same_drawing(&local, &remote) {
            Err(_) => Ok(()),
            Ok(true) => {
                let merged = merge_drawings(None, &local, &remote).map_err(io::Error::other)?;
                if merged != local && !self.replace_local(file, &merged, Some(&local))? {
                    return Ok(());
                }
                if merged == remote {
                    self.state.set(file, &remote, marker)
                } else {
                    self.upload(file, merged)
                }
            }
            Ok(false) => {
                let renamed = self.next_free_name(file)?;
                if self.read_local(file)?.as_deref() != Some(&local[..]) {
                    return Ok(()); // the app wrote it meanwhile
                }
                fs::rename(self.path(file), self.path(&renamed))?;
                self.outcome.changed.push(name_of(&renamed));
                // Writing the cloud's Drawing reports `file` itself.
                self.write_local(file, &remote, marker, None)?;
                self.upload(&renamed, local)
            }
        }
    }

    /// `file` with the lowest number from 2 up that no side uses: "Sketch 2".
    fn next_free_name(&self, file: &str) -> io::Result<String> {
        let stem = name_of(file);
        let taken: BTreeSet<String> = self
            .remote
            .keys()
            .cloned()
            .chain(file_names(self.drawings)?)
            .chain(self.state.files()?)
            .collect();
        Ok((2..)
            .map(|n| format!("{stem} {n}{EXTENSION}"))
            .find(|candidate| !taken.contains(candidate))
            .expect("a free name"))
    }

    fn upload(&mut self, file: &str, bytes: Vec<u8>) -> io::Result<()> {
        if self.outcome.storage_full {
            // Another upload just found the cloud full: don't ask again.
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
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
        let Some(bytes) = self.fetch(file)? else {
            return Ok(()); // deleted since the listing
        };
        self.write_local(file, &bytes, marker, expected)
    }

    /// The cloud's Drawing, if it is still there.
    fn fetch(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
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

    /// Writes `bytes`, the cloud's Drawing with `marker`, here and remembers it.
    fn write_local(
        &mut self,
        file: &str,
        bytes: &[u8],
        marker: &str,
        expected: Option<&[u8]>,
    ) -> io::Result<()> {
        if self.replace_local(file, bytes, expected)? {
            self.state.set(file, bytes, marker)?;
        }
        Ok(())
    }

    /// Writes `bytes` here, unless the local file is not `expected` any more
    /// (the app wrote it meanwhile; it is left for the next step). Whether it
    /// was written.
    fn replace_local(
        &mut self,
        file: &str,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> io::Result<bool> {
        if self.read_local(file)?.as_deref() != expected {
            return Ok(false);
        }
        write_file_atomically(&self.path(file), bytes)?;
        self.outcome.changed.push(name_of(file));
        Ok(true)
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

/// What a step does for one name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Idle,
    /// Gone on both sides: forget it.
    Forget,
    Upload,
    Download,
    /// Present on both sides without history.
    FirstMeeting,
    /// The cloud changed (and maybe the local file too).
    Reconcile {
        local_changed: bool,
    },
    DeleteRemote,
    DeleteLocal,
}

impl Action {
    /// Whether it moves a Drawing's content, as opposed to deleting one or
    /// only updating what is remembered.
    fn is_transfer(self) -> bool {
        matches!(
            self,
            Action::Upload | Action::Download | Action::FirstMeeting | Action::Reconcile { .. }
        )
    }
}

/// The rules of the spec for one name, with L the local file, R the cloud blob
/// (its marker) and B the last-Synced state.
fn decide(local: Option<&[u8]>, remote: Option<&str>, base: Option<&Synced>) -> Action {
    match (local, remote, base) {
        (None, None, _) => Action::Forget,
        (Some(_), None, None) => Action::Upload,
        (None, Some(_), None) => Action::Download,
        // Never Synced under this name, present on both sides.
        (Some(_), Some(_), None) => Action::FirstMeeting,
        (local, remote, Some(base)) => {
            let local_changed = local != Some(&base.bytes[..]);
            let remote_changed = remote != Some(&base.marker[..]);
            match (local, remote) {
                (Some(_), Some(_)) => match (local_changed, remote_changed) {
                    (true, false) => Action::Upload,
                    (_, true) => Action::Reconcile { local_changed },
                    (false, false) => Action::Idle,
                },
                // Deleted here: reaches the cloud, unless it was edited there.
                (None, Some(_)) if remote_changed => Action::Download,
                (None, Some(_)) => Action::DeleteRemote,
                // Deleted there: reaches here, unless it was edited here.
                (Some(_), None) if local_changed => Action::Upload,
                (Some(_), None) => Action::DeleteLocal,
                (None, None) => unreachable!("handled above"),
            }
        }
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
