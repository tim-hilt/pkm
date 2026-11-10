//! The Sync engine (ADR-0011) as a step machine (ADR-0016): one step makes one
//! Context folder of the Device and its cloud folder agree for every Item
//! changed on one side only. The Context's `Merge` hook (ADR-0020) says which
//! files are Items and how two versions become one. Below, an Item is called a
//! Drawing, the first Context's.
//!
//! The engine does no I/O. A `SyncStep` yields one `Request` at a time (list
//! the cloud folder, read a local Drawing, upload a blob, ...) and is handed
//! its result with `resume`; it ends with the `StepOutcome`. A client answers
//! the requests with whatever it has: the native app with file access and a
//! `Storage`, the web app with its storage and `fetch`. Cancelling is the
//! caller not continuing: every write a step requests is whole.
//!
//! A Drawing is its file name, `<name>.excalidraw`, on both sides. For each
//! name the step compares the local file and the cloud blob with what this
//! Device remembers of the last Sync (the Sync state): the bytes then, and the
//! blob's change marker then.
//!
//! A Drawing present on both sides without any history is merged without a
//! base if both are the same Drawing (`are_same_drawing`); otherwise both are
//! kept and the local one is renamed to the next free name ("Sketch 2").
//!
//! A Drawing changed on both sides is merged (`merge_drawings`) and the result
//! written to both. The same merge runs when only one side changed:
//! - the cloud, since the change may be another Device's upload overwriting
//!   ours without having seen it (there is no conditional write): what the
//!   cloud then lacks is uploaded again;
//! - this Device, against the last-Synced bytes the cloud still holds, since
//!   the open Drawing may have saved over newer versions Sync wrote (it keeps
//!   its own version of an Element being dragged): they are not taken back.
//!
//! A poll costs one listing and a scan of the Drawings folder's names and
//! stamps (opaque strings the client supplies, `LocalFile`): a Drawing whose cloud marker and local stamp are the ones
//! remembered is unchanged and is not read.
//!
//! A file that can't be parsed never replaces a Drawing on the other side: it
//! is skipped, and left as it is on both sides.

use std::cell::{RefCell, RefMut};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use std::sync::Arc;

use crate::merge::Merge;
use crate::storage::Blob;

/// How long a file must have been left alone for its stamp to be trusted:
/// file systems with coarse clocks can give two writes the same time.
const SETTLED_MS: u64 = 2_000;

/// The marker remembered for an upload another Device overwrote: no storage
/// gives a blob this one, so the next step finds the blob changed.
const OVERWRITTEN: &str = "\0overwritten";

/// A local Drawing as the client lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalFile {
    /// The file name, `<name>.excalidraw`.
    pub file: String,
    /// Says which file this is, cheaply, and is opaque to the engine: equal
    /// stamps mean the same file. The native client's is inode, modification
    /// time and size; the web app's modification time and size.
    pub stamp: String,
    /// When the file was last modified, in milliseconds since the epoch; 0 if
    /// unknown. A stamp is trusted only once the file has been left alone for
    /// a while.
    pub mtime_ms: u64,
}

/// What this Device remembers of a Drawing at its last Sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    pub bytes: Vec<u8>,
    /// The cloud blob's change marker when it was last seen.
    pub marker: String,
}

/// What the Sync state holds besides the bytes: the cloud marker, and the
/// stamp the local file had when it was found equal to the remembered bytes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Marks {
    pub marker: Option<String>,
    pub stamp: Option<String>,
}

/// What a step asks its client to do. Names are file names. Failures are
/// reported as `io::Error`s: `PermissionDenied` for rejected credentials,
/// `StorageFull` for a full cloud.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// List the cloud folder. Answer `Response::Cloud`.
    ListCloud,
    /// List the local Drawings with their stamps. Answer `Response::Local`.
    ListLocal,
    /// Read a local Drawing, none if there is none. Answer `Response::Bytes`.
    ReadLocal { file: String },
    /// Read the remembered marker and stamp. Answer `Response::Marks`.
    ReadMarks { file: String },
    /// Read the remembered bytes with their marker; none unless both are
    /// there. Answer `Response::Synced`.
    ReadSynced { file: String },
    /// The names with a remembered marker. Answer `Response::Names`.
    ListSynced,
    /// Remember `bytes` and `marker`, and forget the stamp. The marker is
    /// written last: without it the bytes count for nothing. Answer `Done`.
    WriteSynced {
        file: String,
        bytes: Vec<u8>,
        marker: String,
    },
    /// Remember that the local file with `stamp` holds the remembered bytes.
    /// Answer `Done`.
    WriteStamp { file: String, stamp: String },
    /// Forget everything remembered of a name. Answer `Done`.
    ForgetSynced { file: String },
    /// Download a blob, none if it is gone. Answer `Response::Bytes`.
    Download { file: String },
    /// Upload bytes as a blob, replacing any. Answer `Done`.
    Upload { file: String, bytes: Vec<u8> },
    /// Delete a blob. Answer `Done`.
    DeleteCloud { file: String },
    /// Write a local Drawing, whole, only if it still holds `expected` (none:
    /// only if absent). Answer `Response::Happened`.
    WriteLocal {
        file: String,
        bytes: Vec<u8>,
        expected: Option<Vec<u8>>,
    },
    /// Rename a local Drawing only if it still holds `expected`. Answer
    /// `Response::Happened`.
    RenameLocal {
        from: String,
        to: String,
        expected: Vec<u8>,
    },
    /// Delete a local Drawing only if it still holds `expected`. Answer
    /// `Response::Happened`.
    DeleteLocal { file: String, expected: Vec<u8> },
}

/// The result of a `Request`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Response {
    Cloud(Vec<Blob>),
    Local(Vec<LocalFile>),
    Names(Vec<String>),
    Bytes(Option<Vec<u8>>),
    Marks(Marks),
    Synced(Option<Synced>),
    /// Whether a guarded write, rename or delete took place.
    Happened(bool),
    Done,
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

/// What a step does next.
#[derive(Debug)]
pub enum Next {
    /// Answer this and `resume` with the result.
    Request(Request),
    /// The step is over.
    Done(StepOutcome),
}

/// One Sync step. Start it with `resume(None)`, then hand each result to
/// `resume` until it is `Done`.
pub struct SyncStep {
    engine: Option<Pin<Box<dyn Future<Output = ()>>>>,
    shared: Rc<RefCell<Shared>>,
}

/// What the engine and its driver pass to each other.
#[derive(Default)]
struct Shared {
    request: Option<Request>,
    reply: Option<io::Result<Response>>,
    progress: Option<Progress>,
    outcome: StepOutcome,
}

impl SyncStep {
    /// A step starting at `now_ms` (milliseconds since the epoch), which tells
    /// whether a file has been left alone long enough for its stamp to count.
    pub fn new(now_ms: u64, merge: Arc<dyn Merge>) -> Self {
        let shared = Rc::new(RefCell::new(Shared::default()));
        let mut engine = Engine {
            shared: Rc::clone(&shared),
            now_ms,
            merge,
            done: 0,
            total: 0,
            remote: HashMap::new(),
            uploaded: Vec::new(),
        };
        Self {
            engine: Some(Box::pin(async move { engine.run_step().await })),
            shared,
        }
    }

    /// Hands back the result of the last request (none for the first call)
    /// and gives the next one.
    pub fn resume(&mut self, reply: Option<io::Result<Response>>) -> Next {
        let Some(engine) = self.engine.as_mut() else {
            return Next::Done(StepOutcome::default());
        };
        if let Some(reply) = reply {
            self.shared.borrow_mut().reply = Some(reply);
        }
        match engine
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(()) => {
                self.engine = None;
                Next::Done(std::mem::take(&mut self.shared.borrow_mut().outcome))
            }
            Poll::Pending => {
                let request = self.shared.borrow_mut().request.take();
                Next::Request(request.expect("a pending step has asked for something"))
            }
        }
    }

    /// How far the transfers are, once their number is known.
    pub fn progress(&self) -> Option<Progress> {
        self.shared.borrow().progress
    }

    /// Stops the step here, with what it had done so far and `Interrupted` as
    /// its error.
    pub fn cancel(self) -> StepOutcome {
        let mut outcome = std::mem::take(&mut self.shared.borrow_mut().outcome);
        outcome
            .error
            .get_or_insert_with(|| io::Error::from(io::ErrorKind::Interrupted));
        outcome
    }
}

/// Asks the client for something and waits for the result.
struct Exchange<'a> {
    shared: &'a RefCell<Shared>,
    request: Option<Request>,
}

impl Future for Exchange<'_> {
    type Output = io::Result<Response>;

    fn poll(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(request) = self.request.take() {
            self.shared.borrow_mut().request = Some(request);
            return Poll::Pending;
        }
        match self.shared.borrow_mut().reply.take() {
            Some(reply) => Poll::Ready(reply),
            None => Poll::Pending,
        }
    }
}

fn unexpected() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "unexpected reply to a request")
}

struct Engine {
    shared: Rc<RefCell<Shared>>,
    now_ms: u64,
    merge: Arc<dyn Merge>,
    /// Drawings to transfer in this step, and how many are done.
    total: u32,
    done: u32,
    /// File name → change marker of the Drawings in the cloud.
    remote: HashMap<String, String>,
    /// The Drawings put in the cloud, whose new markers are not known yet.
    uploaded: Vec<(String, Vec<u8>)>,
}

impl Engine {
    async fn run_step(&mut self) {
        if let Err(error) = self.run().await {
            self.record(error);
        }
    }

    fn outcome(&self) -> RefMut<'_, StepOutcome> {
        RefMut::map(self.shared.borrow_mut(), |s| &mut s.outcome)
    }

    async fn ask(&self, request: Request) -> io::Result<Response> {
        Exchange {
            shared: &self.shared,
            request: Some(request),
        }
        .await
    }

    async fn run(&mut self) -> io::Result<()> {
        self.remote = self.list_remote().await?;
        // Taken before any Drawing is read: a stamp shows what was read only
        // if the file didn't change between the two.
        let stamps: HashMap<String, LocalFile> = self
            .list_local()
            .await?
            .into_iter()
            .map(|local| (local.file.clone(), local))
            .collect();
        let mut files: BTreeSet<String> = self.remote.keys().cloned().collect();
        files.extend(stamps.keys().cloned());
        files.extend(self.list_synced().await?);

        // A Drawing is read only if it may have changed, and once if nothing
        // is to be done with it.
        let mut pending = Vec::new();
        for file in files {
            let local_file = stamps.get(&file);
            if self.unchanged_by_stamp(&file, local_file).await? {
                continue;
            }
            let (local, remote, base) = self.facts(&file).await?;
            let action = decide(local.as_deref(), remote.as_deref(), base.as_ref());
            if action.is_transfer() {
                self.total += 1;
            }
            if action == Action::Idle {
                if let Some(local_file) = local_file.filter(|f| self.settled(f)) {
                    self.ask(Request::WriteStamp {
                        file: file.clone(),
                        stamp: local_file.stamp.clone(),
                    })
                    .await?;
                }
            } else {
                pending.push(file);
            }
        }
        self.report();

        // Read again: the app may have written a Drawing meanwhile.
        for file in pending {
            if let Err(error) = self.sync_drawing(&file).await {
                self.record(error);
                if self.outcome().access_lost {
                    return Ok(()); // every further call would be rejected too
                }
            }
        }
        self.remember_uploads().await
    }

    /// Whether the file has been unchanged for long enough to rely on. Not
    /// when its time is unknown or in the future.
    fn settled(&self, file: &LocalFile) -> bool {
        file.mtime_ms != 0
            && file.mtime_ms <= self.now_ms
            && self.now_ms - file.mtime_ms >= SETTLED_MS
    }

    /// Whether the Drawing is known to be unchanged on both sides without
    /// reading it: the cloud's marker and the local file's stamp are those it
    /// had when the local bytes were last found equal to the remembered ones.
    async fn unchanged_by_stamp(&self, file: &str, local: Option<&LocalFile>) -> io::Result<bool> {
        let (Some(local), Some(remote)) = (local, self.remote.get(file)) else {
            return Ok(false);
        };
        let marks = self.read_marks(file).await?;
        Ok(marks.stamp.as_deref() == Some(&local.stamp[..])
            && marks.marker.as_deref() == Some(&remote[..]))
    }

    /// Notes what went wrong: lost access and a full cloud are problems of
    /// their own, anything else is an error (offline, most likely).
    fn record(&mut self, error: io::Error) {
        let mut outcome = self.outcome();
        match error.kind() {
            io::ErrorKind::PermissionDenied => outcome.access_lost = true,
            io::ErrorKind::StorageFull => outcome.storage_full = true,
            _ => {
                outcome.error.get_or_insert(error);
            }
        }
    }

    fn report(&self) {
        if self.total > 0 {
            self.shared.borrow_mut().progress = Some(Progress {
                done: self.done,
                total: self.total,
            });
        }
    }

    /// The local file, the cloud blob's marker and the last-Synced state.
    #[allow(clippy::type_complexity)]
    async fn facts(
        &self,
        file: &str,
    ) -> io::Result<(Option<Vec<u8>>, Option<String>, Option<Synced>)> {
        Ok((
            self.read_local(file).await?,
            self.remote.get(file).cloned(),
            self.read_synced(file).await?,
        ))
    }

    async fn list_remote(&self) -> io::Result<HashMap<String, String>> {
        match self.ask(Request::ListCloud).await? {
            Response::Cloud(blobs) => Ok(blobs
                .into_iter()
                .filter(|blob| self.merge.accepts(&blob.name))
                .map(|blob| (blob.name, blob.marker))
                .collect()),
            _ => Err(unexpected()),
        }
    }

    async fn list_local(&self) -> io::Result<Vec<LocalFile>> {
        match self.ask(Request::ListLocal).await? {
            Response::Local(files) => Ok(files
                .into_iter()
                .filter(|f| self.merge.accepts(&f.file))
                .collect()),
            _ => Err(unexpected()),
        }
    }

    async fn list_synced(&self) -> io::Result<Vec<String>> {
        match self.ask(Request::ListSynced).await? {
            Response::Names(names) => Ok(names),
            _ => Err(unexpected()),
        }
    }

    async fn read_marks(&self, file: &str) -> io::Result<Marks> {
        match self.ask(Request::ReadMarks { file: file.into() }).await? {
            Response::Marks(marks) => Ok(marks),
            _ => Err(unexpected()),
        }
    }

    async fn read_synced(&self, file: &str) -> io::Result<Option<Synced>> {
        match self.ask(Request::ReadSynced { file: file.into() }).await? {
            Response::Synced(synced) => Ok(synced),
            _ => Err(unexpected()),
        }
    }

    async fn read_local(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        match self.ask(Request::ReadLocal { file: file.into() }).await? {
            Response::Bytes(bytes) => Ok(bytes),
            _ => Err(unexpected()),
        }
    }

    /// The cloud's Drawing, if it is still there.
    async fn fetch(&self, file: &str) -> io::Result<Option<Vec<u8>>> {
        match self.ask(Request::Download { file: file.into() }).await? {
            Response::Bytes(bytes) => Ok(bytes),
            _ => Err(unexpected()),
        }
    }

    async fn guarded(&self, request: Request) -> io::Result<bool> {
        match self.ask(request).await? {
            Response::Happened(happened) => Ok(happened),
            _ => Err(unexpected()),
        }
    }

    async fn remember(&self, file: &str, bytes: &[u8], marker: &str) -> io::Result<()> {
        self.ask(Request::WriteSynced {
            file: file.into(),
            bytes: bytes.to_vec(),
            marker: marker.into(),
        })
        .await
        .map(drop)
    }

    async fn forget(&self, file: &str) -> io::Result<()> {
        self.ask(Request::ForgetSynced { file: file.into() })
            .await
            .map(drop)
    }

    /// The rules of the spec for one name (`decide`).
    async fn sync_drawing(&mut self, file: &str) -> io::Result<()> {
        let (local, remote, base) = self.facts(file).await?;
        let action = decide(local.as_deref(), remote.as_deref(), base.as_ref());
        let result = match (action, local, remote, base) {
            (Action::Forget, ..) => self.forget(file).await,
            (Action::Upload, Some(local), Some(marker), Some(base)) => {
                self.upload_onto_base(file, &marker, local, &base).await
            }
            (Action::Upload, Some(local), ..) => self.upload(file, local).await,
            (Action::Download, _, Some(marker), _) => self.download(file, &marker, None).await,
            (Action::FirstMeeting, Some(local), Some(marker), _) => {
                self.first_meeting(file, &marker, local).await
            }
            (Action::Reconcile { local_changed }, Some(local), Some(marker), Some(base)) => {
                self.reconcile(file, &marker, local, &base, local_changed)
                    .await
            }
            (Action::DeleteRemote, ..) => {
                match self.ask(Request::DeleteCloud { file: file.into() }).await {
                    Ok(_) => self.forget(file).await,
                    Err(error) => Err(error),
                }
            }
            (Action::DeleteLocal, Some(local), ..) => self.delete_local(file, local).await,
            _ => Ok(()),
        };
        if action.is_transfer() {
            self.done += 1;
            self.report();
        }
        result
    }

    /// The cloud's Drawing changed since the last Sync: merges it with the
    /// local one. What the merge gives goes to the side that lacks it.
    async fn reconcile(
        &mut self,
        file: &str,
        marker: &str,
        local: Vec<u8>,
        base: &Synced,
        local_changed: bool,
    ) -> io::Result<()> {
        let Some(remote) = self.fetch(file).await? else {
            return Ok(()); // deleted since the listing
        };
        match self.merge.merge(Some(&base.bytes), &local, &remote) {
            Some(merged) => {
                self.write_merged(file, marker, &local, &remote, merged)
                    .await
            }
            // Not a Drawing on one side. The cloud's file replaces the local
            // one only if that is unchanged and no Drawing either. Until then
            // every step downloads it again.
            None if local_changed || self.merge.is_item(&local) => Ok(()),
            None => self.write_local(file, &remote, marker, Some(&local)).await,
        }
    }

    /// The local Drawing changed, the cloud's did not: it still holds
    /// `base`. The local one is merged with it before the upload.
    async fn upload_onto_base(
        &mut self,
        file: &str,
        marker: &str,
        local: Vec<u8>,
        base: &Synced,
    ) -> io::Result<()> {
        match self.merge.merge(Some(&base.bytes), &local, &base.bytes) {
            Some(merged) => {
                self.write_merged(file, marker, &local, &base.bytes, merged)
                    .await
            }
            // Not a Drawing on one side: it replaces the cloud's file only if
            // that is no Drawing either.
            None if self.merge.is_item(&base.bytes) => Ok(()),
            None => self.upload(file, local).await,
        }
    }

    /// Writes `merged`, what `local` and `remote` (the cloud's Drawing with
    /// `marker`) merge to, to each side that doesn't hold it yet. Leaves it
    /// for the next step if the app wrote the local file meanwhile.
    async fn write_merged(
        &mut self,
        file: &str,
        marker: &str,
        local: &[u8],
        remote: &[u8],
        merged: Vec<u8>,
    ) -> io::Result<()> {
        if merged != local && !self.replace_local(file, &merged, Some(local)).await? {
            return Ok(());
        }
        if merged == remote {
            self.remember(file, remote, marker).await
        } else {
            self.upload(file, merged).await
        }
    }

    /// A name both sides have, with no history between them. The same
    /// Drawing is merged without a base; unrelated ones are both kept, the
    /// local one under the next free name. A file that is no Drawing is
    /// skipped.
    async fn first_meeting(&mut self, file: &str, marker: &str, local: Vec<u8>) -> io::Result<()> {
        let Some(remote) = self.fetch(file).await? else {
            return Ok(()); // deleted since the listing
        };
        match self.merge.same_item(&local, &remote) {
            None => Ok(()),
            Some(true) => {
                let merged = self
                    .merge
                    .merge(None, &local, &remote)
                    .ok_or_else(|| io::Error::other("not mergeable"))?;
                self.write_merged(file, marker, &local, &remote, merged)
                    .await
            }
            Some(false) => {
                let renamed = self.next_free_name(file).await?;
                let happened = self
                    .guarded(Request::RenameLocal {
                        from: file.into(),
                        to: renamed.clone(),
                        expected: local.clone(),
                    })
                    .await?;
                if !happened {
                    return Ok(()); // the app wrote it meanwhile
                }
                self.outcome().changed.push(self.merge.name_of(&renamed));
                // Writing the cloud's Drawing reports `file` itself.
                self.write_local(file, &remote, marker, None).await?;
                self.upload(&renamed, local).await
            }
        }
    }

    /// `file` with the lowest number from 2 up that no side uses: "Sketch 2".
    async fn next_free_name(&self, file: &str) -> io::Result<String> {
        let mut taken: BTreeSet<String> = self.remote.keys().cloned().collect();
        taken.extend(self.list_local().await?.into_iter().map(|f| f.file));
        taken.extend(self.list_synced().await?);
        Ok((2..)
            .map(|n| self.merge.numbered(file, n))
            .find(|candidate| !taken.contains(candidate))
            .expect("a free name"))
    }

    async fn upload(&mut self, file: &str, bytes: Vec<u8>) -> io::Result<()> {
        if self.outcome().storage_full {
            // Another upload just found the cloud full: don't ask again.
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        self.ask(Request::Upload {
            file: file.into(),
            bytes: bytes.clone(),
        })
        .await?;
        self.uploaded.push((file.to_owned(), bytes));
        Ok(())
    }

    /// Writes the cloud's Drawing here and remembers it. `expected` is the
    /// local file as it was read; if the app wrote it since, it is left for
    /// the next step.
    async fn download(
        &mut self,
        file: &str,
        marker: &str,
        expected: Option<&[u8]>,
    ) -> io::Result<()> {
        let Some(bytes) = self.fetch(file).await? else {
            return Ok(()); // deleted since the listing
        };
        self.write_local(file, &bytes, marker, expected).await
    }

    /// Writes `bytes`, the cloud's Drawing with `marker`, here and remembers it.
    async fn write_local(
        &mut self,
        file: &str,
        bytes: &[u8],
        marker: &str,
        expected: Option<&[u8]>,
    ) -> io::Result<()> {
        if self.replace_local(file, bytes, expected).await? {
            self.remember(file, bytes, marker).await?;
        }
        Ok(())
    }

    /// Writes `bytes` here, unless the local file is not `expected` any more
    /// (the app wrote it meanwhile; it is left for the next step). Whether it
    /// was written.
    async fn replace_local(
        &mut self,
        file: &str,
        bytes: &[u8],
        expected: Option<&[u8]>,
    ) -> io::Result<bool> {
        let happened = self
            .guarded(Request::WriteLocal {
                file: file.into(),
                bytes: bytes.to_vec(),
                expected: expected.map(<[u8]>::to_vec),
            })
            .await?;
        if happened {
            self.outcome().changed.push(self.merge.name_of(file));
        }
        Ok(happened)
    }

    async fn delete_local(&mut self, file: &str, expected: Vec<u8>) -> io::Result<()> {
        let happened = self
            .guarded(Request::DeleteLocal {
                file: file.into(),
                expected,
            })
            .await?;
        if !happened {
            return Ok(()); // the app wrote it meanwhile; the next step decides again
        }
        self.outcome().changed.push(self.merge.name_of(file));
        self.forget(file).await
    }

    /// Remembers the uploads with the markers the cloud gave them, which a
    /// listing tells. One listing for all of them. Without it they are
    /// uploaded again next time, which does no harm.
    ///
    /// Another Device may have overwritten an upload before the listing, and
    /// the marker would then be its blob's. So each blob is read back after
    /// the listing: if it isn't what was uploaded, the upload is remembered
    /// under a marker no blob has, and the next step merges what is in the
    /// cloud with it as the base, as for any change made there.
    async fn remember_uploads(&mut self) -> io::Result<()> {
        if self.uploaded.is_empty() {
            return Ok(());
        }
        let markers = self.list_remote().await?;
        for (file, bytes) in std::mem::take(&mut self.uploaded) {
            let Some(marker) = markers.get(&file) else {
                continue;
            };
            let ours = self.fetch(&file).await?.as_deref() == Some(&bytes[..]);
            self.remember(&file, &bytes, if ours { marker } else { OVERWRITTEN })
                .await?;
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
/// (its marker) and B the last-Synced state. A file missing on one side is
/// deleted on the other only if B says it was Synced; one without B is new
/// and is copied.
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
                // A Delete wins over edits made elsewhere meanwhile.
                (None, Some(_)) => Action::DeleteRemote,
                (Some(_), None) => Action::DeleteLocal,
                (None, None) => unreachable!("handled above"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge::Drawings;

    /// A Device and a cloud in memory, answering a step's requests.
    #[derive(Default)]
    struct Memory {
        cloud: HashMap<String, Vec<u8>>,
        local: HashMap<String, Vec<u8>>,
        marks: HashMap<String, Marks>,
        synced: HashMap<String, Synced>,
        requests: Vec<Request>,
    }

    impl Memory {
        fn marker(bytes: &[u8]) -> String {
            format!(
                "{}-{}",
                bytes.len(),
                bytes.iter().map(|b| *b as u32).sum::<u32>()
            )
        }

        fn answer(&mut self, request: Request) -> Response {
            self.requests.push(request.clone());
            match request {
                Request::ListCloud => Response::Cloud(
                    self.cloud
                        .iter()
                        .map(|(name, bytes)| Blob {
                            name: name.clone(),
                            marker: Self::marker(bytes),
                        })
                        .collect(),
                ),
                Request::ListLocal => Response::Local(
                    self.local
                        .iter()
                        .map(|(file, bytes)| LocalFile {
                            file: file.clone(),
                            stamp: Self::marker(bytes),
                            mtime_ms: 1,
                        })
                        .collect(),
                ),
                Request::ReadLocal { file } => Response::Bytes(self.local.get(&file).cloned()),
                Request::ReadMarks { file } => {
                    Response::Marks(self.marks.get(&file).cloned().unwrap_or_default())
                }
                Request::ReadSynced { file } => Response::Synced(self.synced.get(&file).cloned()),
                Request::ListSynced => Response::Names(self.synced.keys().cloned().collect()),
                Request::WriteSynced {
                    file,
                    bytes,
                    marker,
                } => {
                    self.marks.remove(&file);
                    self.marks.entry(file.clone()).or_default().marker = Some(marker.clone());
                    self.synced.insert(file, Synced { bytes, marker });
                    Response::Done
                }
                Request::WriteStamp { file, stamp } => {
                    self.marks.entry(file).or_default().stamp = Some(stamp);
                    Response::Done
                }
                Request::ForgetSynced { file } => {
                    self.marks.remove(&file);
                    self.synced.remove(&file);
                    Response::Done
                }
                Request::Download { file } => Response::Bytes(self.cloud.get(&file).cloned()),
                Request::Upload { file, bytes } => {
                    self.cloud.insert(file, bytes);
                    Response::Done
                }
                Request::DeleteCloud { file } => {
                    self.cloud.remove(&file);
                    Response::Done
                }
                Request::WriteLocal {
                    file,
                    bytes,
                    expected,
                } => {
                    let happened = self.local.get(&file) == expected.as_ref();
                    if happened {
                        self.local.insert(file, bytes);
                    }
                    Response::Happened(happened)
                }
                Request::RenameLocal { from, to, expected } => {
                    let happened = self.local.get(&from) == Some(&expected);
                    if happened {
                        let bytes = self.local.remove(&from).unwrap();
                        self.local.insert(to, bytes);
                    }
                    Response::Happened(happened)
                }
                Request::DeleteLocal { file, expected } => {
                    let happened = self.local.get(&file) == Some(&expected);
                    if happened {
                        self.local.remove(&file);
                    }
                    Response::Happened(happened)
                }
            }
        }

        /// Runs a step to its end.
        fn step(&mut self, now_ms: u64) -> StepOutcome {
            self.requests.clear();
            let mut step = SyncStep::new(now_ms, Arc::new(Drawings));
            let mut reply = None;
            loop {
                match step.resume(reply.take()) {
                    Next::Done(outcome) => return outcome,
                    Next::Request(request) => reply = Some(Ok(self.answer(request))),
                }
            }
        }
    }

    #[test]
    fn a_new_local_file_is_uploaded_and_the_next_idle_step_only_lists_the_cloud() {
        let mut device = Memory::default();
        device
            .local
            .insert("Sketch.excalidraw".into(), b"not a drawing".to_vec());

        let outcome = device.step(10_000);
        assert!(outcome.error.is_none());
        assert_eq!(device.cloud["Sketch.excalidraw"], b"not a drawing");

        // Settles the stamp, since the file is old enough.
        device.step(10_000);
        device.step(10_000);
        assert_eq!(
            device
                .requests
                .iter()
                .filter(|r| **r == Request::ListCloud)
                .count(),
            1
        );
        assert!(
            !device
                .requests
                .iter()
                .any(|r| matches!(r, Request::Download { .. } | Request::ReadLocal { .. }))
        );
    }

    #[test]
    fn a_file_changed_a_moment_ago_is_read_again_on_the_next_step() {
        let mut device = Memory::default();
        device
            .local
            .insert("Sketch.excalidraw".into(), b"x".to_vec());
        device.step(1_000);
        device.step(1_000);
        device.step(1_000);
        assert!(
            device
                .requests
                .iter()
                .any(|r| matches!(r, Request::ReadLocal { .. }))
        );
    }

    #[test]
    fn a_failing_request_is_the_outcomes_error_and_rejected_credentials_stop_the_step() {
        let mut step = SyncStep::new(0, Arc::new(Drawings));
        assert!(matches!(
            step.resume(None),
            Next::Request(Request::ListCloud)
        ));
        let Next::Done(outcome) = step.resume(Some(Err(io::ErrorKind::PermissionDenied.into())))
        else {
            panic!("the step goes on");
        };
        assert!(outcome.access_lost);
        assert!(outcome.error.is_none());
    }

    #[test]
    fn a_cancelled_step_ends_interrupted() {
        let mut step = SyncStep::new(0, Arc::new(Drawings));
        step.resume(None);
        let outcome = step.cancel();
        assert_eq!(outcome.error.unwrap().kind(), io::ErrorKind::Interrupted);
    }
}
