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
