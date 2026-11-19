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
//!
//! A Context with directories (`Merge::has_directories`, ADR-0031) names its
//! Items by their path, and its directories are Synced too: one the other side
//! lacks is made, one Deleted on a side is Deleted on the other once nothing is
//! left in it. A new Item in a directory the other Device Deleted lands at the
//! top level instead of bringing the directory back.

use std::cell::{RefCell, RefMut};
use std::collections::{BTreeSet, HashMap};
use std::future::Future;
use std::io;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll, Waker};

use std::sync::Arc;

use crate::merge::Merge;
use crate::storage::Tree;

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
    /// List the cloud folder, with its directories at any depth. Answer
    /// `Response::Cloud`.
    ListCloud,
    /// List the local Drawings with their stamps. Answer `Response::Local`.
    ListLocal,
    /// List the local directories at any depth, as paths. Only for a Context
    /// with directories. Answer `Response::Names`.
    ListLocalDirs,
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
    /// Make a local directory and the ones above it. Answer `Done`.
    MakeLocalDir { path: String },
    /// Remove a local directory only if nothing is in it (gone counts as
    /// removed). Answer `Response::Happened`.
    RemoveLocalDir { path: String },
    /// Make a cloud directory and the ones above it. Answer `Done`.
    MakeCloudDir { path: String },
    /// Delete a cloud directory the step found empty. Answer `Done`.
    DeleteCloudDir { path: String },
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
    Cloud(Tree),
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
            dirs: Dirs::default(),
            cloud_blobs: Vec::new(),
            deleted_blobs: BTreeSet::new(),
            deleted_cloud_dirs: BTreeSet::new(),
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
    /// The directories of both sides and what is remembered of them; empty
    /// for a Context without directories.
    dirs: Dirs,
    /// Every blob the cloud listing had, Items or not, and the ones this
    /// step deleted: a directory is empty once all its blobs are deleted.
    cloud_blobs: Vec<String>,
    deleted_blobs: BTreeSet<String>,
    deleted_cloud_dirs: BTreeSet<String>,
}

/// The directories of a Context with directories, as paths from its folder.
#[derive(Default)]
struct Dirs {
    local: BTreeSet<String>,
    cloud: BTreeSet<String>,
    /// The ones both sides had at the last Sync.
    synced: BTreeSet<String>,
}

impl Dirs {
    /// `path` and the directories above it, outermost first.
    fn chain(path: &str) -> impl Iterator<Item = &str> {
        path.match_indices('/')
            .map(|(i, _)| &path[..i])
            .chain([path])
    }

    /// Whether a directory above `file` was Deleted on the side `gone`
    /// lacks, while the other still has it: the Device that made `file`
    /// hasn't seen the Delete.
    fn deleted_above(&self, file: &str, gone: &BTreeSet<String>) -> bool {
        file.rsplit_once('/').is_some_and(|(dir, _)| {
            Self::chain(dir).any(|d| self.synced.contains(d) && !gone.contains(d))
        })
    }
}

/// The key remembering a directory, beside the file names: they never end in `/`.
fn dir_key(dir: &str) -> String {
    format!("{dir}/")
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
        let tree = self.list_cloud().await?;
        self.remote = self.items_of(&tree);
        if self.merge.has_directories() {
            self.note_cloud(tree);
        }
        let synced_names = self.list_synced_names().await?;
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
        files.extend(
            synced_names
                .iter()
                .filter(|name| !name.ends_with('/'))
                .cloned(),
        );
        if self.merge.has_directories() {
            self.dirs.local = self.list_local_dirs().await?;
            self.dirs.synced = synced_names
                .iter()
                .filter_map(|name| name.strip_suffix('/'))
                .map(str::to_owned)
                .collect();
        }

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
                pending.push((file, action));
            }
        }
        self.report();
        // Local deletes last: a Drawing renamed elsewhere (a delete and an
        // add here) is in its new file before its old one goes, so that the
        // app, seeing the old one go, finds it and follows it.
        pending.sort_by_key(|(_, action)| *action == Action::DeleteLocal);

        // Read again: the app may have written a Drawing meanwhile.
        for (file, _) in pending {
            if let Err(error) = self.sync_drawing(&file).await {
                self.record(error);
                if self.outcome().access_lost {
                    return Ok(()); // every further call would be rejected too
                }
            }
        }
        self.sync_dirs().await?;
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

    /// Whether `path`, from the Context folder, is an Item's: a file of the
    /// folder itself, or, with directories, in any visible one.
    fn is_item_path(&self, path: &str) -> bool {
        if self.merge.has_directories() {
            let (_, file) = path.rsplit_once('/').unwrap_or(("", path));
            path.split('/').all(|segment| !segment.starts_with('.')) && self.merge.accepts(file)
        } else {
            !path.contains('/') && self.merge.accepts(path)
        }
    }

    async fn list_cloud(&self) -> io::Result<Tree> {
        match self.ask(Request::ListCloud).await? {
            Response::Cloud(tree) => Ok(tree),
            _ => Err(unexpected()),
        }
    }

    /// The cloud's Items with their markers. The listing goes into
    /// subdirectories (ADR-0031); only a Context with directories has Items
    /// there.
    fn items_of(&self, tree: &Tree) -> HashMap<String, String> {
        tree.blobs
            .iter()
            .filter(|blob| self.is_item_path(&blob.name))
            .map(|blob| (blob.name.clone(), blob.marker.clone()))
            .collect()
    }

    /// Notes the cloud's blobs and directories, for the directory step.
    /// Directories with blobs in them are not listed as such: the paths of
    /// the blobs say it.
    fn note_cloud(&mut self, tree: Tree) {
        let parents = tree
            .blobs
            .iter()
            .filter_map(|blob| blob.name.rsplit_once('/').map(|(dir, _)| dir.to_owned()));
        self.dirs.cloud = parents
            .chain(tree.empty_dirs)
            .flat_map(|dir| Dirs::chain(&dir).map(str::to_owned).collect::<Vec<_>>())
            .filter(|dir| dir.split('/').all(|s| !s.starts_with('.')))
            .collect();
        self.cloud_blobs = tree.blobs.into_iter().map(|blob| blob.name).collect();
    }

    async fn list_local(&self) -> io::Result<Vec<LocalFile>> {
        match self.ask(Request::ListLocal).await? {
            Response::Local(files) => Ok(files
                .into_iter()
                .filter(|f| self.is_item_path(&f.file))
                .collect()),
            _ => Err(unexpected()),
        }
    }

    async fn list_local_dirs(&self) -> io::Result<BTreeSet<String>> {
        match self.ask(Request::ListLocalDirs).await? {
            Response::Names(dirs) => Ok(dirs
                .into_iter()
                .filter(|dir| dir.split('/').all(|s| !s.starts_with('.')))
                .collect()),
            _ => Err(unexpected()),
        }
    }

    /// What is remembered: Items' names, and the directories' as `dir_key`s.
    async fn list_synced_names(&self) -> io::Result<Vec<String>> {
        match self.ask(Request::ListSynced).await? {
            Response::Names(names) => Ok(names),
            _ => Err(unexpected()),
        }
    }

    /// The Items' names that are remembered.
    async fn list_synced(&self) -> io::Result<Vec<String>> {
        let mut names = self.list_synced_names().await?;
        names.retain(|name| !name.ends_with('/'));
        Ok(names)
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
            (Action::Upload, Some(local), None, None)
                if self.dirs.deleted_above(file, &self.dirs.cloud) =>
            {
                self.rehome_local(file, local).await
            }
            (Action::Upload, Some(local), ..) => self.upload(file, local).await,
            (Action::Download, _, Some(_), None)
                if self.dirs.deleted_above(file, &self.dirs.local) =>
            {
                self.rehome_remote(file).await
            }
            (Action::Download, _, Some(marker), _) => self.download(file, &marker, None).await,
            (Action::FirstMeeting, Some(local), Some(marker), _) => {
                self.first_meeting(file, &marker, local).await
            }
            (Action::Reconcile { local_changed }, Some(local), Some(marker), Some(base)) => {
                self.reconcile(file, &marker, local, &base, local_changed)
                    .await
            }
            (Action::DeleteRemote, ..) => {
                self.delete_cloud(file).await?;
                self.forget(file).await
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

    async fn delete_cloud(&mut self, file: &str) -> io::Result<()> {
        self.ask(Request::DeleteCloud { file: file.into() }).await?;
        self.deleted_blobs.insert(file.to_owned());
        Ok(())
    }

    /// A new local Item in a directory another Device Deleted: it moves to the
    /// top level, and is uploaded there. Left where it is if the name is taken.
    async fn rehome_local(&mut self, file: &str, local: Vec<u8>) -> io::Result<()> {
        let top = top_level(file);
        if self.remote.contains_key(top) || self.read_local(top).await?.is_some() {
            return Ok(());
        }
        let happened = self
            .guarded(Request::RenameLocal {
                from: file.into(),
                to: top.into(),
                expected: local.clone(),
            })
            .await?;
        if !happened {
            return Ok(()); // the app wrote it meanwhile
        }
        let changed = [self.merge.name_of(file), self.merge.name_of(top)];
        self.outcome().changed.extend(changed);
        self.upload(top, local).await
    }

    /// A new cloud Item in a directory this Device Deleted: it comes to the
    /// top level here and in the cloud. Left where it is if the name is taken.
    async fn rehome_remote(&mut self, file: &str) -> io::Result<()> {
        let top = top_level(file);
        let Some(bytes) = self.fetch(file).await? else {
            return Ok(()); // deleted since the listing
        };
        if (!self.merge.transfers_unparseable() && !self.merge.is_item(&bytes))
            || self.remote.contains_key(top)
            || !self.replace_local(top, &bytes, None).await?
        {
            return Ok(());
        }
        self.upload(top, bytes).await?;
        self.delete_cloud(file).await
    }

    /// Makes the directories of both sides agree (the Context's directories
    /// as in the module docs), the deepest first, after the Items. Decided on
    /// what the step found; what it did since is only that Items went.
    async fn sync_dirs(&mut self) -> io::Result<()> {
        let mut all: Vec<String> = self
            .dirs
            .local
            .iter()
            .chain(&self.dirs.cloud)
            .chain(&self.dirs.synced)
            .cloned()
            .collect();
        all.sort_by_key(|dir| std::cmp::Reverse(dir.matches('/').count()));
        all.dedup();
        for dir in all {
            let (local, cloud, synced) = (
                self.dirs.local.contains(&dir),
                self.dirs.cloud.contains(&dir),
                self.dirs.synced.contains(&dir),
            );
            match (local, cloud, synced) {
                (true, true, false) => self.remember_dir(&dir).await?,
                (true, false, false) => {
                    self.ask(Request::MakeCloudDir { path: dir.clone() })
                        .await?;
                    self.remember_dir(&dir).await?;
                }
                (false, true, false) => {
                    self.ask(Request::MakeLocalDir { path: dir.clone() })
                        .await?;
                    self.outcome().changed.push(dir_key(&dir));
                    self.remember_dir(&dir).await?;
                }
                (false, false, true) => self.forget(&dir_key(&dir)).await?,
                // Deleted elsewhere: Deleted here once nothing is left in it.
                (true, false, true) => {
                    if self
                        .guarded(Request::RemoveLocalDir { path: dir.clone() })
                        .await?
                    {
                        self.outcome().changed.push(dir_key(&dir));
                        self.forget(&dir_key(&dir)).await?;
                    }
                }
                // Deleted here: Deleted in the cloud once nothing is left in it.
                (false, true, true) if self.cloud_dir_is_empty(&dir) => {
                    self.ask(Request::DeleteCloudDir { path: dir.clone() })
                        .await?;
                    self.forget(&dir_key(&dir)).await?;
                    self.deleted_cloud_dirs.insert(dir);
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Whether every blob and directory the cloud had in `dir` is deleted.
    fn cloud_dir_is_empty(&self, dir: &str) -> bool {
        let inside = |path: &&String| path.strip_prefix(dir).is_some_and(|r| r.starts_with('/'));
        self.cloud_blobs
            .iter()
            .filter(inside)
            .all(|blob| self.deleted_blobs.contains(blob))
            && self
                .dirs
                .cloud
                .iter()
                .filter(inside)
                .all(|sub| self.deleted_cloud_dirs.contains(sub))
    }

    /// Remembers that both sides have the directory. It has no bytes.
    async fn remember_dir(&self, dir: &str) -> io::Result<()> {
        self.remember(&dir_key(dir), &[], "").await
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
            None if local_changed && self.merge.keeps_both_versions() => {
                self.keep_both(file, marker, local, remote).await
            }
            // Not a Drawing on one side. The cloud's file replaces the local
            // one only if that is unchanged and no Drawing either. Until then
            // every step downloads it again.
            None if local_changed || self.merge.is_item(&local) => Ok(()),
            None => self.write_local(file, &remote, marker, Some(&local)).await,
        }
    }

    /// Both sides changed `file` too differently to merge, and this Context
    /// keeps both: the local one gets the next free name, then the cloud's
    /// takes the name they shared.
    async fn keep_both(
        &mut self,
        file: &str,
        marker: &str,
        local: Vec<u8>,
        remote: Vec<u8>,
    ) -> io::Result<()> {
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
        self.write_local(file, &remote, marker, None).await?;
        self.upload(&renamed, local).await
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
        if !self.merge.transfers_unparseable() && !self.merge.is_item(&bytes) {
            return Ok(()); // e.g. a state file Sync can't parse: leave it alone
        }
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
        if !self.merge.transfers_unparseable() && !self.merge.is_item(&bytes) {
            return Ok(()); // e.g. a state file Sync can't parse: leave it alone
        }
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
        let markers = self.items_of(&self.list_cloud().await?);
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

/// The file name of `path`: where it lands at the top level.
fn top_level(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, file)| file)
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
    use crate::storage::Blob;

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
                Request::ListCloud => Response::Cloud(Tree {
                    blobs: self
                        .cloud
                        .iter()
                        .map(|(name, bytes)| Blob {
                            name: name.clone(),
                            marker: Self::marker(bytes),
                        })
                        .collect(),
                    empty_dirs: Vec::new(),
                }),
                Request::ListLocalDirs => Response::Names(Vec::new()),
                Request::MakeLocalDir { .. } | Request::MakeCloudDir { .. } => Response::Done,
                Request::DeleteCloudDir { .. } => Response::Done,
                Request::RemoveLocalDir { .. } => Response::Happened(true),
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
    fn a_rename_from_elsewhere_writes_the_new_file_before_deleting_the_old() {
        let mut device = Memory::default();
        device.local.insert("A.excalidraw".into(), b"x".to_vec());
        device.step(10_000);
        let bytes = device.cloud.remove("A.excalidraw").unwrap();
        device.cloud.insert("B.excalidraw".into(), bytes);

        device.step(10_000);

        let local_writes: Vec<_> = device
            .requests
            .iter()
            .filter_map(|r| match r {
                Request::WriteLocal { file, .. } => Some(format!("write {file}")),
                Request::DeleteLocal { file, .. } => Some(format!("delete {file}")),
                _ => None,
            })
            .collect();
        assert_eq!(local_writes, ["write B.excalidraw", "delete A.excalidraw"]);
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
