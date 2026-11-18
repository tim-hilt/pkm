//! The Tasks store: a folder of `<id>.md` files.

mod filters;
mod images;

pub(crate) use filters::is_filter;
use filters::shown_in;
pub use filters::{Direction, Filter, FilterSort, FilterSortKey};

use crate::files::{DiskFiles, Files};
use crate::query::QueryError;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::{Document, Value};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use pkm_base::tag;
use pkm_base::tags::{Suggestions, Tags};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const DEFAULT_PRIORITY: u8 = 4;
const TAGS: &str = "tags";
const BLOCKED_BY: &str = "blocked-by";
const CREATED: &str = "created";
const SUBTASK_POSITION: &str = "subtask-position";

/// How a Task is Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Done,
    Rejected,
}

/// A Close not yet saved, which `revert` can still take back.
#[derive(Debug, Clone)]
struct Pending {
    /// The Task and its Open Subtasks, which are Deleted with it.
    ids: Vec<ItemId>,
    /// Kept apart for the view while the toast shows (Done stays marked in
    /// place, Rejected leaves at once); it changes nothing once committed.
    closing: Closing,
}

/// A Task another Task is Blocked by.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub id: String,
    pub title: String,
    /// Closed (a pending Done counts): it no longer blocks.
    pub closed: bool,
}

/// An Open Task as the lists show it (a pending Done included).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub title: String,
    /// 1 (highest) to 4, the default.
    pub priority: u8,
    /// Plain text, the file's body; empty if there is none.
    pub description: String,
    /// Valid Tags, in the order they were added.
    pub tags: Vec<String>,
    /// Closed as Done, but the Close is still pending: the list keeps the Task
    /// in place, marked.
    pub done: bool,
    /// The Parent's ID, if this is a Subtask.
    pub parent: Option<String>,
    /// Whether any Task it's Blocked by is Open. Only listings fill this and
    /// `blocked_by`; a Task a change returns has neither.
    pub blocked: bool,
    /// The Tasks it's Blocked by, Open or Closed, in the order they were added;
    /// one that is missing is left out.
    pub blocked_by: Vec<Blocker>,
}

/// The rows of a Filter under one heading: those of the Tag `tag`, or, if
/// `None`, those of no Group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    pub tag: Option<String>,
    pub rows: Vec<Row>,
}

/// A Task as one row of a flat list, with what its markers show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub task: Task,
    /// The Title of its Parent (whose ID is `task.parent`), if it is a Subtask.
    pub parent_title: Option<String>,
    /// How many of its direct Subtasks are still Open; one with a pending
    /// Done no longer counts.
    pub open_subtasks: u32,
}

#[derive(Debug)]
pub enum Error {
    /// The Title is empty or only whitespace.
    EmptyTitle,
    /// A Tag with a forbidden character, or empty, or a misplaced `/`.
    InvalidTag,
    /// A Task can't be Blocked by itself.
    BlockedByItself,
    /// A Task can't be Blocked by its Parent.
    BlockedByParent,
    /// A Task can't be Blocked by one of its Subtasks.
    BlockedBySubtask,
    /// The Blocked by would make Tasks wait on each other.
    BlockedByLoop,
    /// A Filter needs a Title.
    EmptyFilterTitle,
    /// The Query of a Filter can't be read.
    InvalidQuery(QueryError),
    /// The Group by of a Filter names a Tag that doesn't exist.
    UnknownGroupByTag(String),
    /// The Group by of a Filter holds something that isn't a Tag: a Wildcard,
    /// or text a Tag can't be.
    InvalidGroupByTag(String),
    /// A Subtask only moves among its siblings: the Task to move isn't a
    /// Subtask, or the Task to move before isn't its sibling.
    NotASibling,
    Io(io::Error),
}

impl Error {
    /// As an `io::Error`, for the callers that have no other kind.
    fn into_io(self) -> io::Error {
        match self {
            Self::Io(e) => e,
            other => io::Error::other(other.to_string()),
        }
    }
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<io::ErrorKind> for Error {
    fn from(kind: io::ErrorKind) -> Self {
        Self::Io(kind.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => f.write_str("A Task needs a Title."),
            Self::InvalidTag => {
                f.write_str("A Tag uses a-z, 0-9, - and /, and doesn't start or end with /.")
            }
            Self::BlockedByItself => f.write_str("A Task can't be Blocked by itself."),
            Self::BlockedByParent => f.write_str("A Task can't be Blocked by its Parent."),
            Self::BlockedBySubtask => f.write_str("A Task can't be Blocked by its Subtask."),
            Self::BlockedByLoop => f.write_str("Tasks can't be Blocked by each other in a loop."),
            Self::NotASibling => f.write_str("A Subtask moves only among its siblings."),
            Self::EmptyFilterTitle => f.write_str("A Filter needs a Title."),
            Self::InvalidQuery(e) => e.fmt(f),
            Self::UnknownGroupByTag(tag) => write!(f, "Group by: @{tag} doesn't exist."),
            Self::InvalidGroupByTag(text) => write!(f, "Group by: “{text}” isn't a Tag."),
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// The Tasks in one folder (`Tasks` in the app's storage).
#[derive(Debug)]
pub struct Tasks {
    files: Arc<dyn Files>,
    root: PathBuf,
    /// The one Close that can still be reverted.
    pending: Mutex<Option<Pending>>,
    /// Held while the Tags file is read and written back.
    tags_lock: Mutex<()>,
    /// Whole seconds of the last `created` given here.
    last_created: AtomicU64,
}

impl Tasks {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Tasks";

    /// Opens the Tasks in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self::open_in(Arc::new(DiskFiles), root))
    }

    /// The Tasks in `root` on any `files`, which must hold the folder `root`
    /// (an empty one is none).
    pub fn open_in(files: Arc<dyn Files>, root: impl Into<PathBuf>) -> Self {
        Self {
            files,
            root: root.into(),
            pending: Mutex::new(None),
            tags_lock: Mutex::new(()),
            last_created: AtomicU64::new(0),
        }
    }

    /// Creates a Task with `priority` (clamped to 1 to 4) and `tags`, in one
    /// write; Tags that are new join the Tags file. The Title is trimmed and kept to one line (line breaks become spaces). Fails with
    /// `EmptyTitle`, or `InvalidTag` for any invalid Tag, before saving
    /// anything.
    pub fn create(&self, title: &str, priority: u8, tags: &[String]) -> Result<Task, Error> {
        self.insert(&NewTask::checked(title, priority, tags)?, None)
    }

    /// `create` for a Subtask of the Open Task `parent`, at the end of its
    /// Subtask order. It starts with its Parent's Tags (later changes to either
    /// leave the other alone). Fails with `NotFound` if there is no such Open
    /// Task.
    pub fn create_subtask(
        &self,
        parent: &str,
        title: &str,
        priority: u8,
        tags: &[String],
    ) -> Result<Task, Error> {
        let nodes = self.load()?;
        let parent = nodes.get(parent).ok_or(io::ErrorKind::NotFound)?;
        let mut all = parent.task.tags.clone();
        all.extend_from_slice(tags);
        let new = NewTask::checked(title, priority, &all)?;
        let keys: Vec<f64> = (subtask_order(&nodes, &parent.task.id).iter())
            .filter_map(|n| n.subtask_position)
            .collect();
        self.insert(&new, Some((&parent.task.id, end_key(&keys))))
    }

    /// Now in ms, but at least a second after the last Task created here, so
    /// that `created` (whole seconds) keeps the order of creation.
    fn next_created(&self) -> f64 {
        let now = (now_ms() / 1000.0).floor() as u64;
        let next = |last: u64| now.max(last + 1);
        let last = (self.last_created)
            .try_update(Relaxed, Relaxed, |last| Some(next(last)))
            .unwrap_or_else(|last| last);
        next(last) as f64 * 1000.0
    }

    /// Saves a new Task under a fresh Item ID, as a Subtask with its Subtask
    /// position if it has a Parent. No Filter is written: a Task without a place
    /// follows the placed ones by `created`.
    fn insert(&self, new: &NewTask, parent: Option<(&str, f64)>) -> Result<Task, Error> {
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", &new.title);
            file.set_scalar("priority", new.priority.to_string());
            if let Some((parent, position)) = parent {
                file.set_scalar("parent", parent);
                file.set_scalar(SUBTASK_POSITION, position.to_string());
            }
            set_list(&mut file, TAGS, new.tags.clone());
            let now = self.next_created();
            file.set_scalar(CREATED, iso8601(now));
            file.set_scalar("updated", iso8601(now));
            match self.write(&id, &file, Overwrite::No) {
                Ok(()) => return Ok(task_of(&file).ok_or(io::ErrorKind::InvalidData)?),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e.into()),
            }
        }
    }

    /// Sets the Title (as in `create`), Description and Priority of the Open
    /// Task `id`, adds `add_tags` (never removing any; new ones join the Tags
    /// file) and bumps `updated`, all in one write. A Priority outside 1 to 4
    /// is clamped. Every other property, known or not, stays as it was. Fails
    /// with `NotFound` if there is no such Task, or `InvalidTag` for any
    /// invalid Tag, before saving anything.
    pub fn edit(
        &self,
        id: &str,
        title: &str,
        description: &str,
        priority: u8,
        add_tags: &[String],
    ) -> Result<Task, Error> {
        let title = checked_title(title)?;
        let priority = priority.clamp(1, 4);
        let description = description.trim();
        if !add_tags.iter().all(|t| tag::is_valid(t)) {
            return Err(Error::InvalidTag);
        }
        self.modify(id, |file| {
            file.set_scalar("title", &title);
            file.set_scalar("priority", priority.to_string());
            let mut tags = tags_of(file);
            let before = tags.len();
            for tag in add_tags {
                if !tags.contains(tag) {
                    tags.push(tag.clone());
                }
            }
            // Rewritten only when one was added: `tags_of` drops invalid Tags
            // other tools wrote, which an edit of the Title must not remove.
            if tags.len() != before {
                set_list(file, TAGS, tags);
            }
            file.body = if description.is_empty() {
                String::new()
            } else {
                format!("{description}\n")
            };
        })
    }

    /// Moves the Open Subtask `id` before its sibling `before` in its Subtask
    /// order, or after its last sibling if that is `None`; `before` is the
    /// Subtask itself: nothing changes. Usually only its file is written, as
    /// for Tasks (see `place`). Fails with `NotFound` for an unknown Task and
    /// `NotASibling` for a top-level Task or a `before` that isn't a sibling,
    /// before saving anything.
    pub fn move_subtask(&self, id: &str, before: Option<&str>) -> Result<Task, Error> {
        let nodes = self.load()?;
        let moved = nodes.get(id).ok_or(io::ErrorKind::NotFound)?;
        let parent = moved.task.parent.as_deref().ok_or(Error::NotASibling)?;
        if before != Some(id) {
            let anchor = before.map_or(Anchor::End, Anchor::Before);
            self.place_subtask(&nodes, moved, parent, anchor)?;
        }
        Ok(moved.task.clone())
    }

    /// Puts the Subtask `moved` of `parent` at `anchor` among its siblings in
    /// the Subtask order, writing the Task files whose keys change.
    fn place_subtask(
        &self,
        nodes: &Nodes,
        moved: &Node,
        parent: &str,
        anchor: Anchor,
    ) -> Result<(), Error> {
        let mut siblings = subtask_order(nodes, parent);
        let from = siblings.iter().position(|n| n.task.id == moved.task.id);
        siblings.retain(|n| n.task.id != moved.task.id);
        let index_of =
            |id: &str| (siblings.iter().position(|n| n.task.id == id)).ok_or(Error::NotASibling);
        let at = match anchor {
            Anchor::End => siblings.len(),
            Anchor::Before(id) => index_of(id)?,
            Anchor::After(id) => index_of(id)? + 1,
        };
        if from == Some(at) {
            return Ok(());
        }
        let keys: Vec<Option<f64>> = siblings.iter().map(|n| n.subtask_position).collect();
        let placement = place_among(&keys, at);
        for (at, key) in placement.shifted {
            self.set_subtask_position(&siblings[at].task.id, key)?;
        }
        self.set_subtask_position(&moved.task.id, placement.position)
    }

    fn set_subtask_position(&self, id: &str, position: f64) -> Result<(), Error> {
        let key = position.to_string();
        self.modify(id, |file| file.set_scalar(SUBTASK_POSITION, key))
            .map(drop)
    }

    /// Closes the Open Task `id` and its Open Subtasks as one pending change:
    /// nothing is Deleted until `commit`, and `revert` leaves no trace. Done
    /// and Rejected differ only in how the list shows the Task meanwhile. A
    /// Close still pending is committed first. Fails with `NotFound` if there is no such Open Task.
    pub fn close(&self, id: &str, closing: Closing) -> Result<(), Error> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        self.commit()?;
        let nodes = self.load()?;
        nodes.get(id.as_str()).ok_or(io::ErrorKind::NotFound)?;
        *self.pending() = Some(Pending {
            ids: open_subtree(&nodes, id.as_str()),
            closing,
        });
        Ok(())
    }

    /// Drops the pending Close, if any, as if it never happened.
    pub fn revert(&self) {
        self.pending().take();
    }

    /// Deletes the files of the pending Close, if any: the Task and its Open
    /// Subtasks. One Deleted meanwhile, e.g. by Sync, is already as wanted;
    /// where another Device edited one meanwhile, the Delete wins on Sync
    /// (ADR-0020).
    pub fn commit(&self) -> Result<(), Error> {
        let Some(Pending { ids, .. }) = self.pending().take() else {
            return Ok(());
        };
        for id in ids {
            match self.files.remove(&self.path(&id)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
        }
        Ok(())
    }

    fn pending(&self) -> MutexGuard<'_, Option<Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Adds `tag` to the Open Task `id`, as a Tag it already has: nothing
    /// changes. Fails with `InvalidTag` for an invalid Tag and `NotFound` if
    /// there is no such Task.
    pub fn add_tag(&self, id: &str, tag: &str) -> Result<Task, Error> {
        if !tag::is_valid(tag) {
            return Err(Error::InvalidTag);
        }
        self.modify(id, |file| {
            let mut tags = tags_of(file);
            if !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_owned());
                set_list(file, TAGS, tags);
            }
        })
    }

    /// Removes `tag` from the Open Task `id`; a Tag it doesn't have is
    /// ignored. Fails with `NotFound` if there is no such Task.
    pub fn remove_tag(&self, id: &str, tag: &str) -> Result<Task, Error> {
        self.modify(id, |file| {
            let mut tags = tags_of(file);
            tags.retain(|t| t != tag);
            set_list(file, TAGS, tags);
        })
    }

    /// Marks the Open Task `id` as Blocked by the Open Task `blocker`; one it
    /// is Blocked by already stays as it is. Fails with `NotFound` if either is
    /// no Open Task, and with `BlockedByItself`, `BlockedByParent`,
    /// `BlockedBySubtask` or `BlockedByLoop` (`blocker` is itself Blocked by
    /// `id`, directly or through others) for a Blocked by that makes no sense.
    pub fn add_blocker(&self, id: &str, blocker: &str) -> Result<Task, Error> {
        let nodes = self.load()?;
        nodes.get(id).ok_or(io::ErrorKind::NotFound)?;
        nodes.get(blocker).ok_or(io::ErrorKind::NotFound)?;
        if let Some(refusal) = blocker_refusal(&nodes, id, blocker) {
            return Err(refusal);
        }
        self.modify(id, |file| {
            let mut blockers = blockers_of(file);
            if !blockers.iter().any(|b| b == blocker) {
                blockers.push(blocker.to_owned());
                set_list(file, BLOCKED_BY, blockers);
            }
        })
    }

    /// The Open Tasks, as rows, that the Open Task `id` could be
    /// Blocked by: those `add_blocker` accepts, minus the ones it is Blocked by
    /// already. Fails with `NotFound` if there is no such Task.
    pub fn blocker_candidates(&self, id: &str) -> Result<Vec<Row>, Error> {
        let nodes = self.load()?;
        let task = nodes.get(id).ok_or(io::ErrorKind::NotFound)?;
        let mut rows = self.rows()?;
        rows.retain(|row| {
            let candidate = &row.task;
            !candidate.done
                && !task.blocked_by.contains(&candidate.id)
                && blocker_refusal(&nodes, id, &candidate.id).is_none()
        });
        Ok(rows)
    }

    /// Removes `blocker` from what the Open Task `id` is Blocked by; one it
    /// isn't Blocked by is ignored. Fails with `NotFound` if there is no such
    /// Task.
    pub fn remove_blocker(&self, id: &str, blocker: &str) -> Result<Task, Error> {
        self.modify(id, |file| {
            let mut blockers = blockers_of(file);
            blockers.retain(|b| b != blocker);
            set_list(file, BLOCKED_BY, blockers);
        })
    }

    /// What a Tag field offers for the typed `query`, see `Tags::suggest`.
    /// Reads the Tags file only, so a Tag no Task carries any more is found too.
    pub fn suggest_tags(&self, query: &str, leave_out: &[String]) -> io::Result<Suggestions> {
        Ok(self.read_tags().suggest(query, leave_out))
    }

    /// Every known Tag with its Tag Color.
    pub fn tags(&self) -> Tags {
        self.read_tags()
    }

    /// Gives the Tag its Tag Color (`#rrggbb`), or Default for `None`. A Tag
    /// that isn't known is left alone; a malformed color is `InvalidData`.
    pub fn set_tag_color(&self, tag: &str, color: Option<&str>) -> io::Result<()> {
        self.change_tags(|tags| {
            let known = tags.contains(tag);
            if known && !tags.set_color(tag, color, &iso8601(now_ms())) {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "not #rrggbb"));
            }
            Ok(known)
        })
    }

    /// Deletes `tag`: removes it from every Task and every Filter's Group by
    /// (with its places), then forgets it and its Tag Color. In that order, so
    /// that repeating it after an interruption finishes the job.
    pub fn delete_tag(&self, tag: &str) -> io::Result<()> {
        self.remove_tag_everywhere(tag)?;
        self.remove_from_group_bys(tag)?;
        self.forget_tag(tag)
    }

    /// Removes `tag` from every Task, saving each changed file with a new
    /// `updated`. The Tags are left alone; repeating it after an interruption
    /// finishes the job.
    fn remove_tag_everywhere(&self, tag: &str) -> io::Result<()> {
        for listed in self.files.list(&self.root)? {
            let Some(id) = listed.name.strip_suffix(".md").and_then(ItemId::parse) else {
                continue;
            };
            let Some(mut file) = (self.files.read(&self.path(&id)).ok())
                .and_then(|text| Document::parse(&text))
                .filter(is_task)
            else {
                continue;
            };
            let mut tags = file.list(TAGS).unwrap_or_default().to_vec();
            if tags.iter().any(|t| t == tag) {
                tags.retain(|t| t != tag);
                set_list(&mut file, TAGS, tags);
                file.set_scalar("updated", iso8601(now_ms()));
                self.write(&id, &file, Overwrite::Yes)?;
            }
        }
        Ok(())
    }

    /// Forgets `tag` and its Tag Color. A Task or Group by still carrying it
    /// brings it back, grey (ADR-0023).
    fn forget_tag(&self, tag: &str) -> io::Result<()> {
        self.change_tags(|tags| Ok(tags.forget(tag)))
    }

    /// The file of the Open Task `id` and the Task it describes.
    fn read_open(&self, id: &ItemId) -> Result<(Document, Task), Error> {
        let file = Document::parse(&self.files.read(&self.path(id))?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unreadable Task"))?;
        let task = task_of(&file).ok_or(io::ErrorKind::NotFound)?;
        Ok((file, task))
    }

    /// Applies `change` to the Open Task `id`, bumps `updated` and saves; a
    /// change that leaves the file as it was writes nothing, so that saving as
    /// the user goes keeps Sync quiet. Every property `change` leaves alone,
    /// known or not, stays as it was.
    fn modify(&self, id: &str, change: impl FnOnce(&mut Document)) -> Result<Task, Error> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.rewrite(&id, change)?;
        Ok(task_of(&file).ok_or(io::ErrorKind::InvalidData)?)
    }

    /// `modify` for a change that may leave the Task no longer Open.
    fn rewrite(&self, id: &ItemId, change: impl FnOnce(&mut Document)) -> Result<Document, Error> {
        let (mut file, _) = self.read_open(id)?;
        let before = file.render();
        change(&mut file);
        self.drop_missing_blockers(&mut file);
        if file.render() != before {
            file.set_scalar("updated", iso8601(now_ms()));
            self.write(id, &file, Overwrite::Yes)?;
        }
        Ok(file)
    }

    /// Forgets the Blocked by that point at a Task that no longer exists.
    fn drop_missing_blockers(&self, file: &mut Document) {
        let blockers = blockers_of(file);
        let kept: Vec<String> = blockers
            .iter()
            .filter(|b| ItemId::parse(b).is_some_and(|b| self.files.exists(&self.path(&b))))
            .cloned()
            .collect();
        if kept.len() != blockers.len() {
            set_list(file, BLOCKED_BY, kept);
        }
    }

    /// Every Open Task, Subtasks included, as one flat list in the order
    /// created, the Subtasks of one Parent in their Subtask order. A pending
    /// Done shows marked, a pending Rejected not at all. Structure Sync left
    /// broken is repaired here, not in the files.
    pub fn rows(&self) -> io::Result<Vec<Row>> {
        let nodes = self.shown()?;
        Ok(rows_of(
            &nodes,
            shown_in(&nodes, "*", &[], FilterSort::Manual),
        ))
    }

    /// The direct Subtasks of the Open Task `parent`, as
    /// `rows` shows them: Open or pending Done, never Rejected, in their
    /// Subtask order. Empty for a Task that has none or isn't there.
    pub fn subtasks(&self, parent: &str) -> io::Result<Vec<Row>> {
        let nodes = self.shown()?;
        Ok(rows_of(&nodes, subtask_order(&nodes, parent)))
    }

    /// Every Task as the lists show it: repaired, with the pending Close
    /// applied (a pending Done marked, a pending Rejected gone) and Blockers
    /// resolved.
    fn shown(&self) -> io::Result<Nodes> {
        let mut nodes = self.load()?;
        if let Some(Pending { ids, closing, .. }) = self.pending().clone() {
            for id in ids {
                match closing {
                    Closing::Rejected => drop(nodes.remove(id.as_str())),
                    Closing::Done => {
                        if let Some(node) = nodes.get_mut(id.as_str()) {
                            node.task.done = true;
                        }
                    }
                }
            }
        }
        resolve_blockers(&mut nodes);
        Ok(nodes)
    }

    /// Every Task in the folder, repaired. Only `<id>.md` is the Task `id`: a
    /// copy under another name (`<id> 1.md`) would shadow it.
    fn load(&self) -> io::Result<Nodes> {
        let mut read_tags = BTreeSet::new();
        let mut nodes = Nodes::new();
        for listed in self.files.list(&self.root)? {
            if !listed.name.ends_with(".md") {
                continue;
            }
            let Some((node, tags)) = read_node(self.files.as_ref(), &self.root.join(&listed.name))
            else {
                continue;
            };
            read_tags.extend(tags);
            nodes.insert(node.task.id.clone(), node);
        }
        self.remember_tags(read_tags)?;
        repair(&mut nodes);
        Ok(nodes)
    }

    fn path(&self, id: &ItemId) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    /// Writes `<id>.md` whole and adds its Tags to the Tags. Unless told to
    /// overwrite, fails with `AlreadyExists` rather than replacing a Task.
    fn write(&self, id: &ItemId, file: &Document, overwrite: Overwrite) -> io::Result<()> {
        let path = self.path(id);
        if overwrite == Overwrite::No && self.files.exists(&path) {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        self.files.write(&path, &file.render())?;
        self.remember_tags(tags_of(file))
    }

    /// Where the Tags are: beside the Tasks folder, like every Context's.
    fn tags_path(&self) -> PathBuf {
        self.root.parent().unwrap_or(Path::new("")).join(Tags::FILE)
    }

    /// The Tags; none if there is no file or it isn't one.
    fn read_tags(&self) -> Tags {
        let text = self.files.read(&self.tags_path()).unwrap_or_default();
        Tags::parse(&text)
    }

    /// Adds the valid `found` Tags to the Tags file, leaving known ones as
    /// they are; reads nothing if none are found, writes nothing if all are
    /// known.
    fn remember_tags(&self, found: impl IntoIterator<Item = String>) -> io::Result<()> {
        let mut found = found.into_iter().peekable();
        if found.peek().is_none() {
            return Ok(());
        }
        let updated = iso8601(now_ms());
        self.change_tags(|tags| {
            let mut changed = false;
            for tag in found {
                changed |= tags.add(&tag, &updated);
            }
            Ok(changed)
        })
    }

    /// Reads the Tags, lets `change` change them and writes them back if it
    /// says it did, all under `tags_lock`.
    fn change_tags(&self, change: impl FnOnce(&mut Tags) -> io::Result<bool>) -> io::Result<()> {
        let _held = self
            .tags_lock
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let mut tags = self.read_tags();
        if change(&mut tags)? {
            self.files.write(&self.tags_path(), &tags.render())?;
        }
        Ok(())
    }
}

#[derive(PartialEq, Eq)]
enum Overwrite {
    Yes,
    No,
}

#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub(crate) task: Task,
    /// When it was created (ISO 8601 UTC); `None` only until `load` gives it one.
    pub(crate) created: Option<String>,
    /// Its fractional key in its Subtask order; `None` for a Task that has
    /// none (a top-level one, or a Subtask another tool wrote), which follows
    /// the keyed siblings.
    pub(crate) subtask_position: Option<f64>,
    /// The IDs the file lists, whether or not such a Task exists.
    blocked_by: Vec<String>,
}

type Nodes = HashMap<String, Node>;

/// The Task in the file at `path`, if it is one and is named by its id, and
/// the valid Tags the file carries.
fn read_node(files: &dyn Files, path: &Path) -> Option<(Node, Vec<String>)> {
    let text = files.read(path).ok()?;
    let file = Document::parse(&text)?;
    let node = node_of(&file)?;
    (path.file_stem()? == node.task.id.as_str()).then(|| (node, tags_of(&file)))
}

/// The Task a file describes; `None` for a malformed one.
fn node_of(file: &Document) -> Option<Node> {
    let tags = tags_of(file);
    let task = Task {
        id: file.scalar("id")?.to_owned(),
        title: file.scalar("title")?.to_owned(),
        priority: file
            .scalar("priority")
            .and_then(|p| p.parse().ok())
            .filter(|p| (1..=4).contains(p))
            .unwrap_or(DEFAULT_PRIORITY),
        description: file.body.trim().to_owned(),
        tags,
        done: false,
        parent: file
            .scalar("parent")
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
        blocked: false,
        blocked_by: Vec::new(),
    };
    Some(Node {
        task,
        created: file.scalar(CREATED).map(str::to_owned),
        subtask_position: (file.scalar(SUBTASK_POSITION))
            .and_then(|p| p.parse().ok())
            .filter(|p: &f64| p.is_finite()),
        blocked_by: blockers_of(file),
    })
}

/// Whether `file` is a Task.
pub(crate) fn is_task(file: &Document) -> bool {
    node_of(file).is_some()
}

/// The Task a file describes; `None` for a malformed one.
fn task_of(file: &Document) -> Option<Task> {
    node_of(file).map(|n| n.task)
}

/// Fixes what no Device allowed but Sync can leave (ADR-0021): a Task whose
/// Parent is missing becomes top-level, and a Parent cycle is cut at the
/// smallest Item ID.
fn repair(nodes: &mut Nodes) {
    let orphans: Vec<String> = nodes
        .values()
        .filter(|n| (n.task.parent.as_ref()).is_some_and(|parent| !nodes.contains_key(parent)))
        .map(|n| n.task.id.clone())
        .collect();
    for id in orphans {
        nodes.entry(id).and_modify(|n| n.task.parent = None);
    }

    let mut ids: Vec<String> = nodes.keys().cloned().collect();
    ids.sort();
    for start in ids {
        let mut seen = vec![start];
        while let Some(parent) = nodes[&seen[seen.len() - 1]].task.parent.clone() {
            if let Some(at) = seen.iter().position(|s| *s == parent) {
                let smallest = seen[at..].iter().min().cloned().unwrap_or_default();
                nodes.entry(smallest).and_modify(|n| n.task.parent = None);
                break;
            }
            seen.push(parent);
        }
    }
}

/// Fills `blocked` and `blocked_by` of every Task. A Blocked by pointing at a
/// missing Task is left out. Loops need no repair: each Task looks only at the
/// Tasks it is Blocked by.
fn resolve_blockers(nodes: &mut Nodes) {
    let known: HashMap<String, Blocker> = nodes
        .values()
        .map(|n| {
            let blocker = Blocker {
                id: n.task.id.clone(),
                title: n.task.title.clone(),
                closed: n.task.done,
            };
            (blocker.id.clone(), blocker)
        })
        .collect();
    for node in nodes.values_mut() {
        node.task.blocked_by = node
            .blocked_by
            .iter()
            .filter_map(|id| known.get(id).cloned())
            .collect();
        node.task.blocked = node.task.blocked_by.iter().any(|b| !b.closed);
    }
}

/// Why the Task `id` can't be Blocked by the Task `blocker`, if so.
fn blocker_refusal(nodes: &Nodes, id: &str, blocker: &str) -> Option<Error> {
    let parent_of = |id: &str| nodes.get(id).and_then(|n| n.task.parent.as_deref());
    if id == blocker {
        Some(Error::BlockedByItself)
    } else if parent_of(id) == Some(blocker) {
        Some(Error::BlockedByParent)
    } else if parent_of(blocker) == Some(id) {
        Some(Error::BlockedBySubtask)
    } else if waits_for(nodes, blocker, id) {
        Some(Error::BlockedByLoop)
    } else {
        None
    }
}

/// Whether `from` is Blocked by `target`, directly or through other Tasks.
/// Ends on loops Sync left that don't pass `target`.
fn waits_for(nodes: &Nodes, from: &str, target: &str) -> bool {
    let mut seen = HashSet::from([from]);
    let mut todo = vec![from];
    while let Some(current) = todo.pop() {
        let Some(node) = nodes.get(current) else {
            continue;
        };
        for next in &node.blocked_by {
            if next == target {
                return true;
            }
            if seen.insert(next) {
                todo.push(next);
            }
        }
    }
    false
}

/// `root`, then its Subtasks at any depth.
fn open_subtree(nodes: &Nodes, root: &str) -> Vec<ItemId> {
    let mut ids = vec![root.to_owned()];
    let mut next = 0;
    while next < ids.len() {
        let parent = ids[next].clone();
        next += 1;
        ids.extend(
            (nodes.values())
                .filter(|n| n.task.parent.as_deref() == Some(&parent))
                .map(|n| n.task.id.clone()),
        );
    }
    ids.iter().filter_map(|id| ItemId::parse(id)).collect()
}

/// The direct Subtasks of `parent` in their Subtask order: by key, those
/// without one last, then by Item ID.
fn subtask_order<'a>(nodes: &'a Nodes, parent: &str) -> Vec<&'a Node> {
    let mut found: Vec<&Node> = (nodes.values())
        .filter(|n| n.task.parent.as_deref() == Some(parent))
        .collect();
    found.sort_by(|a, b| by_subtask_order(a, b));
    found
}

fn by_subtask_order(a: &Node, b: &Node) -> Ordering {
    let key = |n: &Node| n.subtask_position.unwrap_or(f64::INFINITY);
    compare_positions(key(a), key(b)).then_with(|| a.task.id.cmp(&b.task.id))
}

/// `list` with the rows the Subtasks of each Parent take refilled with them in
/// their Subtask order; no other row moves. This is how a Filter, which
/// decides the rows, keeps the one Subtask order.
pub(super) fn in_subtask_order(mut list: Vec<&Node>) -> Vec<&Node> {
    let mut rows: HashMap<&str, Vec<usize>> = HashMap::new();
    for (at, node) in list.iter().enumerate() {
        if let Some(parent) = node.task.parent.as_deref() {
            rows.entry(parent).or_default().push(at);
        }
    }
    for rows in rows.into_values() {
        let mut siblings: Vec<&Node> = rows.iter().map(|&at| list[at]).collect();
        siblings.sort_by(|a, b| by_subtask_order(a, b));
        for (at, node) in rows.into_iter().zip(siblings) {
            list[at] = node;
        }
    }
    list
}

/// Where the Subtask `place_subtask` places goes among its siblings.
enum Anchor<'a> {
    End,
    Before(&'a str),
    After(&'a str),
}

/// `found` as Rows, with the markers counted over all `nodes`.
fn rows_of(nodes: &Nodes, found: Vec<&Node>) -> Vec<Row> {
    let counts = open_subtask_counts(nodes);
    found
        .into_iter()
        .map(|node| {
            let task = &node.task;
            let open_subtasks = counts.get(task.id.as_str()).copied().unwrap_or_default();
            Row {
                task: task.clone(),
                parent_title: (task.parent.as_ref())
                    .and_then(|p| nodes.get(p))
                    .map(|p| p.task.title.clone()),
                open_subtasks,
            }
        })
        .collect()
}

/// The key after the last of `keys`.
fn end_key(keys: &[f64]) -> f64 {
    keys.last().map_or(1.0, |k| k + 1.0)
}

/// A new fractional key for an entry, and the entries that had to move for
/// it, each by its index with its new key.
struct Placement {
    position: f64,
    shifted: Vec<(usize, f64)>,
}

/// The `Placement` that puts an entry at index `at` of the ascending `keys`
/// of a list (without the entry): of Filters, or of the Tasks in one Filter.
/// Usually only the entry gets a key, between its neighbours'. Where they
/// share one there is no key between them, so the entries of that run from
/// `at` on move up, spread below the next key, ahead of which the entry goes.
fn place(keys: &[f64], at: usize) -> Placement {
    let alone = |position| Placement {
        position,
        shifted: Vec::new(),
    };
    let Some(&next) = keys.get(at) else {
        return alone(end_key(keys));
    };
    let Some(prev) = at.checked_sub(1).map(|prev| keys[prev]) else {
        return alone(next - 1.0);
    };
    if prev < next {
        return alone(f64::midpoint(prev, next));
    }
    let run = keys[at..].iter().take_while(|&&k| k == next).count();
    let high = keys.get(at + run).map_or(prev + 1.0, |&k| k);
    let step = (high - prev) / (run + 2) as f64;
    Placement {
        position: prev + step,
        shifted: (0..run)
            .map(|n| (at + n, prev + step * (n + 2) as f64))
            .collect(),
    }
}

/// `place` for entries that may not have a usable run of keys: where one has
/// no key, or they are out of order (a Filter refilled with Subtasks), every
/// entry is numbered anew, 1, 2, 3 ... with the new one at index `at`.
fn place_among(keys: &[Option<f64>], at: usize) -> Placement {
    let ascending: Option<Vec<f64>> = keys.iter().copied().collect();
    match ascending.filter(|k| k.is_sorted()) {
        Some(keys) => place(&keys, at),
        None => Placement {
            position: (at + 1) as f64,
            shifted: (0..keys.len())
                .map(|i| (i, (i + 1 + usize::from(i >= at)) as f64))
                .collect(),
        },
    }
}

/// Fractional keys in order; one that is no number ties.
fn compare_positions(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// What a Parent's marker shows, by Parent ID: how many of its direct
/// Subtasks are still Open (a pending Done is no longer).
fn open_subtask_counts(nodes: &Nodes) -> HashMap<&str, u32> {
    let mut counts = HashMap::new();
    for node in nodes.values().filter(|n| !n.task.done) {
        if let Some(parent) = node.task.parent.as_deref() {
            *counts.entry(parent).or_default() += 1;
        }
    }
    counts
}

/// The valid Tags in a file; others, which only other tools could have
/// written, are ignored.
fn tags_of(file: &Document) -> Vec<String> {
    let tags = file.list("tags").unwrap_or_default();
    tags.iter().filter(|t| tag::is_valid(t)).cloned().collect()
}

/// The IDs a file is Blocked by, whether or not such Tasks exist.
fn blockers_of(file: &Document) -> Vec<String> {
    file.list(BLOCKED_BY).unwrap_or_default().to_vec()
}

/// An empty list is no property at all.
fn set_list(file: &mut Document, key: &str, items: Vec<String>) {
    if items.is_empty() {
        file.remove(key);
    } else {
        file.set(key, Value::List(items));
    }
}

/// The checked properties of a Task about to be created.
struct NewTask {
    title: String,
    priority: u8,
    /// Without duplicates, in the order given.
    tags: Vec<String>,
}

impl NewTask {
    fn checked(title: &str, priority: u8, tags: &[String]) -> Result<Self, Error> {
        let mut unique: Vec<String> = Vec::new();
        for tag in tags {
            if !tag::is_valid(tag) {
                return Err(Error::InvalidTag);
            }
            if !unique.contains(tag) {
                unique.push(tag.clone());
            }
        }
        Ok(Self {
            title: checked_title(title)?,
            priority: priority.clamp(1, 4),
            tags: unique,
        })
    }
}

fn checked_title(title: &str) -> Result<String, Error> {
    let title = one_line(title);
    if title.is_empty() {
        Err(Error::EmptyTitle)
    } else {
        Ok(title)
    }
}

fn one_line(title: &str) -> String {
    title
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder that removes itself.
    pub(super) struct Scratch(pub(super) PathBuf);

    impl Scratch {
        pub(super) fn new() -> Self {
            let mut entropy = Entropy::from_system();
            let dir = std::env::temp_dir().join(format!("pkm-tasks-{}", ItemId::new(&mut entropy)));
            Self(dir.join("Tasks"))
        }

        pub(super) fn store(&self) -> Tasks {
            Tasks::open(&self.0).unwrap()
        }

        fn tags_file(&self) -> PathBuf {
            self.0.parent().unwrap().join(Tags::FILE)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(self.0.parent().unwrap()));
        }
    }

    /// The Tasks of a list, as `rows` lists them.
    fn list_of(tasks: &Tasks) -> Vec<Task> {
        let rows = tasks.rows().unwrap();
        rows.into_iter().map(|r| r.task).collect()
    }

    fn titles(tasks: &Tasks) -> Vec<String> {
        list_of(tasks).into_iter().map(|t| t.title).collect()
    }

    #[test]
    fn a_new_task_with_the_tag_backlog_is_listed_like_any_other() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks.create("Buy milk", 4, &[]).unwrap();
        tasks
            .create("Learn Rust", 4, &tag_names(&["backlog"]))
            .unwrap();
        assert_eq!(titles(&tasks), ["Buy milk", "Learn Rust"]);
    }

    #[test]
    fn tasks_are_listed_in_the_order_they_were_created() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["one", "two", "three"] {
            tasks.create(title, 4, &[]).unwrap();
        }
        assert_eq!(titles(&tasks), ["one", "two", "three"]);
    }

    #[test]
    fn a_new_task_has_priority_4() {
        let dir = Scratch::new();
        let task = dir.store().create("x", 4, &[]).unwrap();
        assert_eq!(task.priority, 4);
    }

    fn tag_names(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|t| (*t).to_owned()).collect()
    }

    #[test]
    fn a_task_is_created_with_its_priority_and_tags_in_one_write() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let tags = tag_names(&["bug", "work", "bug"]);
        let task = tasks.create("Fix login", 1, &tags).unwrap();
        assert_eq!(
            (task.priority, task.tags.clone()),
            (1, tag_names(&["bug", "work"]))
        );
        assert_eq!(list_of(&tasks), std::slice::from_ref(&task));
        let text = file_of(&dir, &task);
        assert!(
            text.contains("priority: 1") && text.contains("tags:"),
            "{text}"
        );
    }

    #[test]
    fn the_tags_of_a_new_task_join_the_tags_file() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks.create("a", 4, &tag_names(&["new", "work"])).unwrap();
        let known: Vec<_> = tasks.tags().list().into_iter().map(|e| e.name).collect();
        assert_eq!(known, ["new", "work"]);
    }

    #[test]
    fn an_invalid_tag_or_an_empty_title_creates_nothing() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let bad = tag_names(&["ok", "Bad"]);
        assert!(matches!(tasks.create("a", 4, &bad), Err(Error::InvalidTag)));
        assert!(matches!(tasks.create(" ", 1, &[]), Err(Error::EmptyTitle)));
        assert!(titles(&tasks).is_empty());
        assert!(tasks.tags().list().is_empty());
    }

    #[test]
    fn a_priority_outside_1_to_4_is_clamped_on_create() {
        let dir = Scratch::new();
        let tasks = dir.store();
        assert_eq!(tasks.create("a", 0, &[]).unwrap().priority, 1);
        assert_eq!(tasks.create("b", 9, &[]).unwrap().priority, 4);
    }

    #[test]
    fn a_subtask_is_created_with_its_priority_and_tags() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &tag_names(&["work"])).unwrap();
        let sub = tasks
            .create_subtask(&parent.id, "s", 2, &tag_names(&["home"]))
            .unwrap();
        assert_eq!(sub.priority, 2);
        assert_eq!(sub.tags, tag_names(&["work", "home"]));
        assert!(tasks.tags().contains("home"));
    }

    #[test]
    fn a_subtask_starts_with_its_parents_tags_and_keeps_its_own_afterwards() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks
            .create("p", 4, &tag_names(&["work", "backlog"]))
            .unwrap();
        let sub = tasks.create_subtask(&parent.id, "s", 4, &[]).unwrap();
        assert_eq!(sub.tags, tag_names(&["work", "backlog"]));
        tasks.add_tag(&parent.id, "home").unwrap();
        tasks.remove_tag(&parent.id, "work").unwrap();
        let subtasks = tasks.subtasks(&parent.id).unwrap();
        assert_eq!(subtasks[0].task.tags, tag_names(&["work", "backlog"]));
    }

    #[test]
    fn an_empty_or_blank_title_is_refused() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["", "   ", "\n\t \n"] {
            assert!(matches!(
                tasks.create(title, 4, &[]),
                Err(Error::EmptyTitle)
            ));
        }
        assert!(titles(&tasks).is_empty());
    }

    #[test]
    fn the_title_is_trimmed_and_kept_to_one_line() {
        let dir = Scratch::new();
        let task = dir
            .store()
            .create("  Buy\n  milk \r\n\n and eggs ", 4, &[])
            .unwrap();
        assert_eq!(task.title, "Buy milk and eggs");
    }

    #[test]
    fn a_task_is_a_markdown_file_named_by_its_id_with_front_matter() {
        let dir = Scratch::new();
        let task = dir
            .store()
            .create("Buy: milk", 4, &tag_names(&["backlog"]))
            .unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        let file = Document::parse(&text).unwrap();
        assert_eq!(file.scalar("id"), Some(task.id.as_str()));
        assert_eq!(file.scalar("title"), Some("Buy: milk"));
        assert_eq!(file.scalar("priority"), Some("4"));
        assert_eq!(file.list("tags"), Some(&["backlog".to_owned()][..]));
        assert!(
            ["state", "position", "order"]
                .iter()
                .all(|k| file.scalar(k).is_none())
        );
        assert!(file.scalar("created").is_some());
        let updated = file.scalar("updated").unwrap();
        assert!(updated.len() == 20 && updated.ends_with('Z'), "{updated}");
    }

    #[test]
    fn tasks_persist_across_opens() {
        let dir = Scratch::new();
        dir.store().create("kept", 4, &[]).unwrap();
        assert_eq!(titles(&dir.store()), ["kept"]);
    }

    fn edit_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        let task = tasks.create("Buy milk", 4, &[]).unwrap();
        (tasks, task)
    }

    #[test]
    fn editing_sets_title_description_and_priority() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let edited = tasks
            .edit(&task.id, " Buy oat milk ", "Two\nlitres", 1, &[])
            .unwrap();
        assert_eq!(edited.title, "Buy oat milk");
        assert_eq!(edited.description, "Two\nlitres");
        assert_eq!(edited.priority, 1);
        assert_eq!(list_of(&tasks), [edited]);
    }

    #[test]
    fn editing_adds_tags_in_one_write_and_keeps_the_others() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["a"]);
        let add = ["a".to_owned(), "new/b".to_owned()];
        let edited = tasks.edit(&task.id, "n", "", 2, &add).unwrap();
        assert_eq!((edited.title.as_str(), edited.priority), ("n", 2));
        assert_eq!(edited.tags, ["a", "new/b"]);
        assert!(known(&tasks, "new").contains(&"new/b".to_owned()));
    }

    #[test]
    fn editing_with_an_invalid_tag_changes_nothing() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let add = ["ok".to_owned(), "Bad".to_owned()];
        assert!(matches!(
            tasks.edit(&task.id, "n", "", 1, &add),
            Err(Error::InvalidTag)
        ));
        assert_eq!(list_of(&tasks), [task]);
        assert!(known(&tasks, "ok").is_empty());
    }

    #[test]
    fn editing_refuses_an_empty_title_and_changes_nothing() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        assert!(matches!(
            tasks.edit(&task.id, "  ", "x", 1, &[]),
            Err(Error::EmptyTitle)
        ));
        assert_eq!(list_of(&tasks), [task]);
    }

    #[test]
    fn editing_an_unknown_task_fails() {
        let dir = Scratch::new();
        let (tasks, _) = edit_fixture(&dir);
        for id in ["abcdefgh", "../x"] {
            assert!(matches!(
                tasks.edit(id, "t", "", 4, &[]),
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
            ));
        }
    }

    #[test]
    fn editing_clamps_the_priority() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        assert_eq!(tasks.edit(&task.id, "t", "", 0, &[]).unwrap().priority, 1);
        assert_eq!(tasks.edit(&task.id, "t", "", 9, &[]).unwrap().priority, 4);
    }

    #[test]
    fn the_description_is_the_files_body() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        tasks.edit(&task.id, "t", "  details  ", 4, &[]).unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.ends_with("---\ndetails\n"), "{text}");
        tasks.edit(&task.id, "t", "", 4, &[]).unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.ends_with("---\n"), "{text}");
    }

    #[test]
    fn editing_keeps_unknown_properties_and_bumps_updated() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let path = dir.0.join(format!("{}.md", task.id));
        let mut file = Document::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        file.set_scalar("updated", "2000-01-01T00:00:00Z");
        file.set_scalar("due", "2030-05-01");
        file.set(
            "aliases",
            pkm_base::front_matter::Value::List(vec!["a".into(), "b".into()]),
        );
        fs::write(&path, file.render()).unwrap();

        tasks.edit(&task.id, "New", "body", 2, &[]).unwrap();

        let file = Document::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(file.scalar("due"), Some("2030-05-01"));
        assert_eq!(
            file.list("aliases"),
            Some(&["a".to_owned(), "b".to_owned()][..])
        );
        assert_ne!(file.scalar("updated"), Some("2000-01-01T00:00:00Z"));
    }

    #[test]
    fn a_change_that_changes_nothing_writes_nothing() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        tasks.edit(&task.id, "t", "body", 2, &[]).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let mut file = Document::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        file.set_scalar("updated", "2000-01-01T00:00:00Z");
        fs::write(&path, file.render()).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        tasks.edit(&task.id, " t ", "body ", 2, &[]).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        tasks.remove_tag(&task.id, "away").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), before);
    }

    #[test]
    fn files_that_are_no_tasks_are_not_listed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        fs::write(dir.0.join("notes.txt"), "x").unwrap();
        fs::write(dir.0.join("broken.md"), "---\ntitle: no end").unwrap();
        fs::write(dir.0.join("plain.md"), "no front matter").unwrap();
        assert!(titles(&tasks).is_empty());
    }

    /// The Tags suggested for `query`, leaving none out.
    fn known(tasks: &Tasks, query: &str) -> Vec<String> {
        tasks.suggest_tags(query, &[]).unwrap().matches
    }

    fn tagged(tasks: &Tasks, tags: &[&str]) -> Task {
        let task = tasks.create("t", 4, &[]).unwrap();
        for tag in tags {
            tasks.add_tag(&task.id, tag).unwrap();
        }
        list_of(tasks).pop().unwrap()
    }

    #[test]
    fn tags_are_added_in_order_and_stored_as_a_block_list() {
        let dir = Scratch::new();
        let task = tagged(&dir.store(), &["home", "work/x"]);
        assert_eq!(task.tags, ["home", "work/x"]);
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.contains("tags:\n  - home\n  - work/x\n"), "{text}");
        assert_eq!(list_of(&dir.store()), [task]);
    }

    #[test]
    fn adding_a_tag_twice_keeps_one() {
        let dir = Scratch::new();
        assert_eq!(tagged(&dir.store(), &["a", "a"]).tags, ["a"]);
    }

    #[test]
    fn invalid_tags_are_refused_and_change_nothing() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["ok"]);
        for tag in ["", "Home", "a b", "/a", "a/", "a//b"] {
            assert!(matches!(
                tasks.add_tag(&task.id, tag),
                Err(Error::InvalidTag)
            ));
        }
        assert_eq!(list_of(&tasks), [task]);
    }

    #[test]
    fn removing_a_tag_keeps_the_others_and_ignores_a_missing_one() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["a", "b"]);
        assert_eq!(tasks.remove_tag(&task.id, "a").unwrap().tags, ["b"]);
        assert_eq!(tasks.remove_tag(&task.id, "zzz").unwrap().tags, ["b"]);
        let task = tasks.remove_tag(&task.id, "b").unwrap();
        assert!(task.tags.is_empty());
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(!text.contains("tags"), "{text}");
    }

    #[test]
    fn tagging_an_unknown_task_fails() {
        let dir = Scratch::new();
        assert!(matches!(
            dir.store().add_tag("abcdefgh", "a"),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn editing_and_tagging_keep_each_other() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["a"]);
        assert_eq!(tasks.edit(&task.id, "n", "d", 2, &[]).unwrap().tags, ["a"]);
        let task = tasks.add_tag(&task.id, "b").unwrap();
        assert_eq!((task.title.as_str(), task.priority), ("n", 2));
    }

    #[test]
    fn known_tags_are_suggested_sorted_and_once() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tagged(&tasks, &["work/b", "home"]);
        tagged(&tasks, &["work/a", "home", "w"]);
        assert_eq!(known(&tasks, "w"), ["w", "work/a", "work/b"]);
        assert_eq!(known(&tasks, "").len(), 4);
        assert!(known(&tasks, "x").is_empty());
    }

    #[test]
    fn a_tag_color_is_set_and_reset() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tagged(&tasks, &["home"]);
        tasks.set_tag_color("home", Some("#d0021b")).unwrap();
        assert_eq!(tasks.tags().color("home"), Some("#d0021b"));
        tasks.set_tag_color("home", None).unwrap();
        assert_eq!(tasks.tags().color("home"), None);
        tasks.set_tag_color("unknown", Some("#d0021b")).unwrap();
        assert!(!tasks.tags().contains("unknown"));
        assert!(tasks.set_tag_color("home", Some("red")).is_err());
    }

    #[test]
    fn a_tag_no_task_carries_any_more_is_still_suggested() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["gone", "kept"]);
        tasks.remove_tag(&task.id, "gone").unwrap();
        assert_eq!(known(&tasks, ""), ["gone", "kept"]);
    }

    #[test]
    fn tags_are_kept_in_the_tags_file_beside_the_context_folders() {
        let dir = Scratch::new();
        tagged(&dir.store(), &["home"]);
        let text = fs::read_to_string(dir.tags_file()).unwrap();
        assert_eq!(Tags::parse(&text).suggest("", &[]).matches, ["home"]);
    }

    #[test]
    fn saving_an_edit_adds_the_tags_the_file_carries() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path).unwrap();
        fs::write(
            &path,
            text.replace("priority:", "tags:\n  - hand\npriority:"),
        )
        .unwrap();
        assert!(known(&tasks, "").is_empty()); // not read yet
        tasks.edit(&task.id, "new", "", 4, &[]).unwrap();
        assert_eq!(known(&tasks, ""), ["hand"]);
    }

    #[test]
    fn reading_a_task_file_with_an_unknown_tag_adds_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", 4, &[]).unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path).unwrap();
        let text = text.replace("priority:", "tags:\n  - synced\n  - Bad Tag\npriority:");
        fs::write(&path, text).unwrap();
        list_of(&tasks);
        assert_eq!(known(&tasks, ""), ["synced"]);
    }

    #[test]
    fn deleting_a_tag_rewrites_only_the_tasks_carrying_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let open = tagged(&tasks, &["home", "work"]);
        let other = tagged(&tasks, &["home"]);
        let path = |t: &Task| dir.0.join(format!("{}.md", t.id));
        let stamp = |t: &Task| {
            let mut file = Document::parse(&fs::read_to_string(path(t)).unwrap()).unwrap();
            file.set_scalar("updated", "2000-01-01T00:00:00Z");
            fs::write(path(t), file.render()).unwrap();
        };
        for task in [&open, &other] {
            stamp(task);
        }
        let other_text = fs::read_to_string(path(&other)).unwrap();

        tasks.delete_tag("work").unwrap();

        let file = Document::parse(&fs::read_to_string(path(&open)).unwrap()).unwrap();
        assert_eq!(tags_of(&file), ["home"]);
        assert_ne!(file.scalar("updated"), Some("2000-01-01T00:00:00Z"));
        assert_eq!(fs::read_to_string(path(&other)).unwrap(), other_text);
    }

    #[test]
    fn a_deleted_tag_is_gone_and_starts_grey_when_added_again() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["work"]);
        tasks.set_tag_color("work", Some("#d0021b")).unwrap();
        tasks.delete_tag("work").unwrap();
        assert!(!tasks.tags().contains("work"));
        tasks.add_tag(&task.id, "work").unwrap();
        assert!(tasks.tags().contains("work"));
        assert_eq!(tasks.tags().color("work"), None);
    }

    fn closing_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        tasks.create("other", 4, &[]).unwrap();
        let task = tasks.create("Buy milk", 4, &[]).unwrap();
        (tasks, task)
    }

    fn file_of(dir: &Scratch, task: &Task) -> String {
        fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap()
    }

    #[test]
    fn a_pending_done_task_stays_in_place_marked_done() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Done).unwrap();
        let listed = list_of(&tasks);
        assert_eq!(
            listed.iter().map(|t| t.done).collect::<Vec<_>>(),
            [false, true]
        );
        assert_eq!(listed[1].id, task.id);
    }

    #[test]
    fn a_pending_rejected_task_is_left_out() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Rejected).unwrap();
        assert_eq!(titles(&tasks), ["other"]);
    }

    #[test]
    fn reverting_leaves_every_file_as_it_was() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        let before = file_of(&dir, &task);
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.revert();
        tasks.commit().unwrap();
        assert_eq!(list_of(&tasks).len(), 2);
        assert!(list_of(&tasks).iter().all(|t| !t.done));
        assert_eq!(file_of(&dir, &task), before);
    }

    #[test]
    fn nothing_is_deleted_before_the_commit() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        let before = file_of(&dir, &task);
        tasks.close(&task.id, Closing::Done).unwrap();
        assert_eq!(file_of(&dir, &task), before);
    }

    #[test]
    fn done_and_rejected_both_delete_the_file_on_commit() {
        for closing in [Closing::Done, Closing::Rejected] {
            let dir = Scratch::new();
            let (tasks, task) = closing_fixture(&dir);
            tasks.close(&task.id, closing).unwrap();
            tasks.commit().unwrap();
            assert!(!dir.0.join(format!("{}.md", task.id)).exists());
            assert_eq!(titles(&dir.store()), ["other"]);
            tasks.commit().unwrap(); // nothing pending: nothing to do
        }
    }

    #[test]
    fn closing_leaves_the_other_tasks_files_as_they_were() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        let other = list_of(&tasks).remove(0);
        let before = file_of(&dir, &other);
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert_eq!(file_of(&dir, &other), before);
    }

    #[test]
    fn a_new_close_commits_the_previous_one_first() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let first = tasks.create("first", 4, &[]).unwrap();
        let second = tasks.create("second", 4, &[]).unwrap();
        tasks.close(&first.id, Closing::Done).unwrap();
        tasks.close(&second.id, Closing::Rejected).unwrap();
        assert!(!dir.0.join(format!("{}.md", first.id)).exists());
        assert!(dir.0.join(format!("{}.md", second.id)).exists());
        tasks.revert();
        assert_eq!(titles(&tasks), ["second"]);
    }

    #[test]
    fn closing_an_unknown_task_fails() {
        let dir = Scratch::new();
        assert!(dir.store().close("aaaaaaaaaaaa", Closing::Done).is_err());
    }

    #[test]
    fn a_pending_close_deletes_the_file_whatever_was_edited_meanwhile() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.edit(&task.id, "Renamed", "", 1, &[]).unwrap();
        tasks.commit().unwrap();
        assert!(!dir.0.join(format!("{}.md", task.id)).exists());
    }

    #[test]
    fn a_task_blocked_by_a_closed_task_is_no_longer_blocked() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let blocker = tasks.create("blocker", 4, &[]).unwrap();
        let waiting = tasks.create("waiting", 4, &[]).unwrap();
        tasks.add_blocker(&waiting.id, &blocker.id).unwrap();
        assert!(list_of(&tasks)[1].blocked);
        tasks.close(&blocker.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        let rows = tasks.rows().unwrap();
        assert!(!rows[0].task.blocked && rows[0].task.blocked_by.is_empty());
    }

    fn subtask(tasks: &Tasks, parent: &Task, title: &str) -> Task {
        tasks.create_subtask(&parent.id, title, 4, &[]).unwrap()
    }

    /// Each row's Title with its Parent's, in list order.
    fn tree(tasks: &Tasks) -> Vec<(String, Option<String>)> {
        let rows = tasks.rows().unwrap();
        rows.into_iter()
            .map(|r| (r.task.title, r.parent_title))
            .collect()
    }

    /// `(title, parent)` as `tree` lists them.
    fn sub(title: &str, parent: Option<&str>) -> (String, Option<String>) {
        (title.to_owned(), parent.map(str::to_owned))
    }

    /// A raw Task file, as Sync or another tool could leave it: `state` is
    /// "backlog" for one tagged so, anything else for an Active one.
    fn raw(dir: &Scratch, id: &str, state: &str, parent: Option<&str>) {
        let parent = parent.map_or(String::new(), |p| format!("parent: {p}\n"));
        let state = if state == "backlog" {
            "tags:\n  - backlog\n"
        } else {
            ""
        };
        let text = format!(
            "---\nid: {id}\ntitle: {id}\npriority: 4\n{state}{parent}updated: 2000-01-01T00:00:00Z\n---\n"
        );
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join(format!("{id}.md")), text).unwrap();
    }

    #[test]
    fn a_subtask_is_listed_under_its_parent_in_the_parents_list() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("parent", 4, &tag_names(&["backlog"])).unwrap();
        let child = subtask(&tasks, &parent, "child");
        let grandchild = subtask(&tasks, &child, "grandchild");
        subtask(&tasks, &parent, "second");
        assert_eq!(grandchild.parent.as_deref(), Some(child.id.as_str()));
        assert_eq!(
            tree(&tasks),
            [
                sub("parent", None),
                sub("child", Some("parent")),
                sub("grandchild", Some("child")),
                sub("second", Some("parent")),
            ]
        );
    }

    #[test]
    fn a_subtask_needs_an_open_parent_and_a_title() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        assert!(matches!(
            tasks.create_subtask(&parent.id, " ", 4, &[]),
            Err(Error::EmptyTitle)
        ));
        assert!(matches!(
            tasks.create_subtask("aaaaaaaaaaaa", "x", 4, &[]),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn closing_a_parent_closes_its_open_subtasks_in_one_pending_change() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        let grandchild = subtask(&tasks, &child, "g");
        let before = file_of(&dir, &child);
        tasks.close(&parent.id, Closing::Done).unwrap();
        assert!(list_of(&tasks).iter().all(|t| t.done));
        tasks.revert();
        tasks.commit().unwrap();
        assert_eq!(file_of(&dir, &child), before);

        tasks.close(&parent.id, Closing::Rejected).unwrap();
        assert!(list_of(&tasks).is_empty());
        tasks.commit().unwrap();
        for task in [&parent, &child, &grandchild] {
            assert!(!dir.0.join(format!("{}.md", task.id)).exists());
        }
    }

    #[test]
    fn a_closed_subtask_leaves_the_list_once_the_close_is_committed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let open = subtask(&tasks, &parent, "open");
        let gone = subtask(&tasks, &parent, "gone");
        tasks.close(&gone.id, Closing::Rejected).unwrap();
        assert_eq!(titles(&tasks), ["p", "open"]);
        tasks.commit().unwrap();
        assert_eq!(list_of(&tasks)[1].id, open.id);
        assert_eq!(titles(&tasks), ["p", "open"]);
        tasks.close(&parent.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(list_of(&tasks).is_empty());
    }

    #[test]
    fn an_orphan_becomes_top_level() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "aaaaaaaaaaaa", "backlog", Some("zzzzzzzzzzzz"));
        assert_eq!(titles(&tasks), ["aaaaaaaaaaaa"]);
    }

    #[test]
    fn a_parent_cycle_is_cut_at_the_smallest_item_id() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "bbbbbbbbbbbb", "active", Some("cccccccccccc"));
        raw(&dir, "cccccccccccc", "active", Some("aaaaaaaaaaaa"));
        raw(&dir, "aaaaaaaaaaaa", "active", Some("bbbbbbbbbbbb"));
        raw(&dir, "dddddddddddd", "active", Some("dddddddddddd"));
        let parents: HashMap<_, _> = tree(&tasks).into_iter().collect();
        let parent = |id: &str| parents[id].clone();
        assert_eq!(parent("aaaaaaaaaaaa"), None);
        assert_eq!(parent("dddddddddddd"), None);
        assert_eq!(parent("cccccccccccc").as_deref(), Some("aaaaaaaaaaaa"));
        assert_eq!(parent("bbbbbbbbbbbb").as_deref(), Some("cccccccccccc"));
    }

    #[test]
    fn a_copy_of_a_task_file_under_another_name_is_ignored() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("original", 4, &[]).unwrap();
        let copy = file_of(&dir, &task).replace("original", "copy");
        for name in [format!("{} 1.md", task.id), format!("~{}.md", task.id)] {
            fs::write(dir.0.join(name), &copy).unwrap();
        }
        tasks.edit(&task.id, "edited", "", 4, &[]).unwrap();
        assert_eq!(titles(&tasks), ["edited"]);
    }

    #[test]
    fn committing_skips_a_subtask_removed_meanwhile() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let gone = subtask(&tasks, &parent, "gone");
        let kept = subtask(&tasks, &parent, "kept");
        tasks.close(&parent.id, Closing::Done).unwrap();
        fs::remove_file(dir.0.join(format!("{}.md", gone.id))).unwrap();
        tasks.commit().unwrap();
        assert!(!dir.0.join(format!("{}.md", parent.id)).exists());
        assert!(!dir.0.join(format!("{}.md", kept.id)).exists());
        assert!(titles(&tasks).is_empty());
    }

    fn move_fixture(dir: &Scratch) -> (Tasks, [Task; 3]) {
        let tasks = dir.store();
        let [a, b, c] = ["a", "b", "c"].map(|t| tasks.create(t, 4, &[]).unwrap());
        (tasks, [a, b, c])
    }

    #[test]
    fn equal_keys_sort_by_item_id() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "cccccccccccc", "active", None);
        raw(&dir, "aaaaaaaaaaaa", "active", None);
        raw(&dir, "bbbbbbbbbbbb", "active", None);
        assert_eq!(
            titles(&tasks),
            ["aaaaaaaaaaaa", "bbbbbbbbbbbb", "cccccccccccc"]
        );
    }

    /// A raw Subtask file with the Subtask position `key`, as Sync could leave
    /// several Subtasks with one key.
    fn raw_subtask(dir: &Scratch, id: &str, parent: &str, key: &str) {
        let text = format!(
            "---\nid: {id}\ntitle: {id}\nparent: {parent}\nsubtask-position: {key}\nupdated: 2000-01-01T00:00:00Z\n---\n"
        );
        fs::write(dir.0.join(format!("{id}.md")), text).unwrap();
    }

    #[test]
    fn a_subtask_moved_among_equal_keys_lands_in_place_and_the_others_shift() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let [b, c, d] = ["bbbbbbbbbbbb", "cccccccccccc", "dddddddddddd"];
        for id in [b, c, d] {
            raw_subtask(&dir, id, &parent.id, "1");
        }
        assert_eq!(subtask_titles(&tasks, &parent), [b, c, d]);
        tasks.move_subtask(d, Some(c)).unwrap();
        assert_eq!(subtask_titles(&dir.store(), &parent), [b, d, c]);
        tasks.move_subtask(b, None).unwrap();
        assert_eq!(subtask_titles(&dir.store(), &parent), [d, c, b]);
    }

    #[test]
    fn a_subtask_without_a_key_follows_the_keyed_ones_and_gets_one_on_a_move() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let (a, b) = ("aaaaaaaaaaaa", "bbbbbbbbbbbb");
        raw(&dir, a, "active", Some(&parent.id));
        raw_subtask(&dir, b, &parent.id, "7");
        let last = subtask(&tasks, &parent, "last");
        assert_eq!(subtask_titles(&tasks, &parent), [b, "last", a]);
        tasks.move_subtask(&last.id, Some(b)).unwrap();
        assert_eq!(subtask_titles(&tasks, &parent), ["last", b, a]);
    }

    fn subtask_titles(tasks: &Tasks, parent: &Task) -> Vec<String> {
        let rows = tasks.subtasks(&parent.id).unwrap();
        rows.into_iter().map(|r| r.task.title).collect()
    }

    #[test]
    fn a_subtask_moves_before_a_sibling_or_after_the_last() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let [x, y, z] = ["x", "y", "z"].map(|t| subtask(&tasks, &a, t));
        let moved = tasks.move_subtask(&z.id, Some(&x.id)).unwrap();
        assert_eq!(moved.parent.as_deref(), Some(a.id.as_str()));
        assert_eq!(subtask_titles(&tasks, &a), ["z", "x", "y"]);
        tasks.move_subtask(&z.id, None).unwrap();
        assert_eq!(subtask_titles(&tasks, &a), ["x", "y", "z"]);
        tasks.move_subtask(&y.id, Some(&y.id)).unwrap();
        assert_eq!(subtask_titles(&dir.store(), &a), ["x", "y", "z"]);
    }

    #[test]
    fn a_subtask_move_rewrites_only_the_moved_subtask() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let [x, y, z] = ["x", "y", "z"].map(|t| subtask(&tasks, &a, t));
        let before = (file_of(&dir, &a), file_of(&dir, &x), file_of(&dir, &y));
        tasks.move_subtask(&z.id, Some(&y.id)).unwrap();
        assert_eq!(
            (file_of(&dir, &a), file_of(&dir, &x), file_of(&dir, &y)),
            before
        );
    }

    #[test]
    fn a_subtask_is_refused_a_move_before_a_non_sibling() {
        let dir = Scratch::new();
        let (tasks, [a, b, _]) = move_fixture(&dir);
        let x = subtask(&tasks, &a, "x");
        let y = subtask(&tasks, &b, "y");
        let nested = subtask(&tasks, &x, "nested");
        for other in [&a, &b, &y, &nested] {
            assert!(matches!(
                tasks.move_subtask(&x.id, Some(&other.id)),
                Err(Error::NotASibling)
            ));
        }
        assert!(matches!(
            tasks.move_subtask(&a.id, None),
            Err(Error::NotASibling)
        ));
        assert_eq!(subtask_titles(&tasks, &a), ["x"]);
    }

    fn created_of(dir: &Scratch, id: &str) -> Option<String> {
        let text = fs::read_to_string(dir.0.join(format!("{id}.md"))).unwrap();
        Document::parse(&text)?.scalar("created").map(str::to_owned)
    }

    #[test]
    fn a_new_task_gets_created_once_and_edits_never_change_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("a", 4, &[]).unwrap();
        let created = created_of(&dir, &task.id).unwrap();
        assert!(created.len() == 20 && created.ends_with('Z'), "{created}");
        tasks.edit(&task.id, "b", "", 1, &[]).unwrap();
        tasks.add_tag(&task.id, "x").unwrap();
        assert_eq!(created_of(&dir, &task.id), Some(created));
    }

    #[test]
    fn a_new_task_or_subtask_follows_the_others_by_created() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        tasks.create("next", 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "child");
        subtask(&tasks, &child, "grandchild");
        tasks.create("last", 4, &[]).unwrap();
        let rows: Vec<_> = tasks
            .rows()
            .unwrap()
            .into_iter()
            .map(|r| r.task.title)
            .collect();
        assert_eq!(rows, ["p", "next", "child", "grandchild", "last"]);
    }

    fn two_tasks(tasks: &Tasks) -> (Task, Task) {
        (
            tasks.create("a", 4, &[]).unwrap(),
            tasks.create("b", 4, &tag_names(&["backlog"])).unwrap(),
        )
    }

    fn listed(tasks: &Tasks, id: &str) -> Task {
        list_of(tasks).into_iter().find(|t| t.id == id).unwrap()
    }

    #[test]
    fn blocked_by_is_stored_as_a_block_list_and_listed_with_its_blockers() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        tasks.add_blocker(&a.id, &b.id).unwrap();
        tasks.add_blocker(&a.id, &b.id).unwrap(); // twice keeps one
        assert!(file_of(&dir, &a).contains(&format!("blocked-by:\n  - {}\n", b.id)));
        let a = listed(&dir.store(), &a.id);
        assert!(a.blocked);
        assert_eq!(
            a.blocked_by,
            [Blocker {
                id: b.id.clone(),
                title: "b".into(),
                closed: false
            }]
        );
        assert!(!listed(&tasks, &b.id).blocked);
    }

    #[test]
    fn removing_a_blocker_unblocks_and_drops_the_property() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        tasks.add_blocker(&a.id, &b.id).unwrap();
        tasks.remove_blocker(&a.id, "zzzzzzzzzzzz").unwrap(); // not blocked by: ignored
        tasks.remove_blocker(&a.id, &b.id).unwrap();
        assert!(!listed(&tasks, &a.id).blocked);
        assert!(!file_of(&dir, &a).contains("blocked-by"));
    }

    #[test]
    fn a_task_stays_blocked_until_every_blocker_is_closed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        let c = tasks.create("c", 4, &[]).unwrap();
        tasks.add_blocker(&a.id, &b.id).unwrap();
        tasks.add_blocker(&a.id, &c.id).unwrap();
        tasks.close(&b.id, Closing::Done).unwrap();
        assert!(listed(&tasks, &a.id).blocked);
        tasks.commit().unwrap();
        assert!(listed(&tasks, &a.id).blocked);
        tasks.close(&c.id, Closing::Rejected).unwrap();
        assert!(!listed(&tasks, &a.id).blocked);
        tasks.revert();
        assert!(listed(&tasks, &a.id).blocked);
        tasks.close(&c.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        let a = listed(&tasks, &a.id);
        assert!(!a.blocked);
        assert!(a.blocked_by.is_empty());
    }

    #[test]
    fn a_blocked_task_can_still_be_closed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        tasks.add_blocker(&a.id, &b.id).unwrap();
        tasks.close(&a.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(!dir.0.join(format!("{}.md", a.id)).exists());
    }

    #[test]
    fn senseless_blocked_by_is_refused_and_changes_nothing() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        let sub = subtask(&tasks, &a, "sub");
        tasks.add_blocker(&a.id, &b.id).unwrap();
        let before = file_of(&dir, &a);
        assert!(matches!(
            tasks.add_blocker(&a.id, &a.id),
            Err(Error::BlockedByItself)
        ));
        assert!(matches!(
            tasks.add_blocker(&sub.id, &a.id),
            Err(Error::BlockedByParent)
        ));
        assert!(matches!(
            tasks.add_blocker(&a.id, &sub.id),
            Err(Error::BlockedBySubtask)
        ));
        assert!(matches!(
            tasks.add_blocker(&b.id, &a.id),
            Err(Error::BlockedByLoop)
        ));
        let c = tasks.create("c", 4, &[]).unwrap();
        tasks.add_blocker(&b.id, &c.id).unwrap();
        assert!(matches!(
            tasks.add_blocker(&c.id, &a.id),
            Err(Error::BlockedByLoop)
        ));
        assert_eq!(file_of(&dir, &a), before);
    }

    #[test]
    fn blocking_by_an_unknown_task_fails() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, _) = two_tasks(&tasks);
        assert!(matches!(
            tasks.add_blocker(&a.id, "aaaaaaaaaaaa"),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn blocker_candidates_leave_out_what_add_blocker_refuses() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let a = tasks.create("a", 4, &[]).unwrap();
        let sub = tasks.create_subtask(&a.id, "sub", 4, &[]).unwrap();
        let b = tasks.create("b", 4, &tag_names(&["backlog"])).unwrap();
        let c = tasks.create("c", 4, &[]).unwrap();
        let d = tasks.create("d", 4, &tag_names(&["backlog"])).unwrap();
        let e = tasks.create("e", 4, &[]).unwrap();
        let gone = tasks.create("gone", 4, &[]).unwrap();
        tasks.close(&gone.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        tasks.add_blocker(&b.id, &a.id).unwrap(); // direct loop for a
        tasks.add_blocker(&c.id, &b.id).unwrap(); // longer loop for a
        tasks.add_blocker(&a.id, &e.id).unwrap(); // current Blocker
        let titles = |id: &str| -> Vec<String> {
            let rows = tasks.blocker_candidates(id).unwrap();
            rows.into_iter().map(|r| r.task.title).collect()
        };
        assert_eq!(titles(&a.id), ["d"]);
        assert_eq!(titles(&sub.id), ["b", "c", "d", "e"]); // not a, its Parent
        assert_eq!(titles(&d.id), ["a", "sub", "b", "c", "e"]);
        assert!(matches!(
            tasks.blocker_candidates(&gone.id),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    /// `raw` with a `blocked-by` list.
    fn raw_blocked(dir: &Scratch, id: &str, state: &str, blockers: &[&str]) {
        raw(dir, id, state, None);
        let path = dir.0.join(format!("{id}.md"));
        let list: String = blockers.iter().map(|b| format!("  - {b}\n")).collect();
        let text = fs::read_to_string(&path).unwrap().replace(
            "priority: 4\n",
            &format!("priority: 4\nblocked-by:\n{list}"),
        );
        fs::write(path, text).unwrap();
    }

    #[test]
    fn a_blocked_by_pointing_at_a_missing_task_is_ignored_and_dropped_at_the_next_save() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw_blocked(&dir, "aaaaaaaaaaaa", "active", &["zzzzzzzzzzzz"]);
        let a = listed(&tasks, "aaaaaaaaaaaa");
        assert!(!a.blocked && a.blocked_by.is_empty());
        assert!(file_of(&dir, &a).contains("zzzzzzzzzzzz"));
        tasks.edit("aaaaaaaaaaaa", "t", "", 4, &[]).unwrap();
        assert!(!file_of(&dir, &a).contains("blocked-by"));
    }

    #[test]
    fn a_blocked_by_loop_left_by_sync_is_tolerated() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw_blocked(&dir, "aaaaaaaaaaaa", "active", &["bbbbbbbbbbbb"]);
        raw_blocked(&dir, "bbbbbbbbbbbb", "active", &["aaaaaaaaaaaa"]);
        raw_blocked(&dir, "cccccccccccc", "active", &["cccccccccccc"]);
        let listed = list_of(&tasks);
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|t| t.blocked));
        let d = tasks.create("d", 4, &[]).unwrap();
        tasks.add_blocker(&d.id, "aaaaaaaaaaaa").unwrap();
        assert!(matches!(
            tasks.add_blocker("bbbbbbbbbbbb", &d.id),
            Err(Error::BlockedByLoop)
        ));
    }

    #[test]
    fn a_file_changed_added_or_removed_outside_the_app_shows_up() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let kept = tasks.create("kept", 4, &[]).unwrap();
        let other = tasks.create("other", 4, &[]).unwrap();
        assert_eq!(titles(&tasks), ["kept", "other"]);

        let path = dir.0.join(format!("{}.md", kept.id));
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("kept", "changed");
        fs::write(&path, text).unwrap();
        raw(&dir, "aaaaaaaaaaaa", "active", None);
        fs::remove_file(dir.0.join(format!("{}.md", other.id))).unwrap();
        let sorted = |mut titles: Vec<String>| {
            titles.sort();
            titles
        };
        assert_eq!(sorted(titles(&tasks)), ["aaaaaaaaaaaa", "changed"]);
        assert_eq!(sorted(titles(&dir.store())), ["aaaaaaaaaaaa", "changed"]);
    }

    #[test]
    fn rows_list_each_task_before_its_subtasks() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("parent", 4, &[]).unwrap();
        tasks.create_subtask(&parent.id, "child", 4, &[]).unwrap();
        tasks.create("other", 4, &[]).unwrap();
        let rows: Vec<_> = tasks
            .rows()
            .unwrap()
            .into_iter()
            .map(|r| (r.task.title, r.parent_title, r.open_subtasks))
            .collect();
        let some = |t: &str| Some(t.to_owned());
        assert_eq!(
            rows,
            [
                ("parent".to_owned(), None, 1),
                ("child".to_owned(), some("parent"), 0),
                ("other".to_owned(), None, 0)
            ]
        );
    }

    #[test]
    fn subtasks_are_the_direct_open_ones_in_order_with_list_markers() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let top = tasks.create("top", 4, &tag_names(&["backlog"])).unwrap();
        let first = subtask(&tasks, &top, "first");
        let second = subtask(&tasks, &top, "second");
        subtask(&tasks, &first, "grandchild");
        let done = subtask(&tasks, &top, "done");
        let rejected = subtask(&tasks, &top, "rejected");
        tasks.create("other", 4, &tag_names(&["backlog"])).unwrap();
        tasks.close(&rejected.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        tasks.close(&done.id, Closing::Done).unwrap();
        let rows = tasks.subtasks(&top.id).unwrap();
        let shown: Vec<_> = rows
            .iter()
            .map(|r| (r.task.title.as_str(), r.task.done, r.open_subtasks))
            .collect();
        assert_eq!(
            shown,
            [("first", false, 1), ("second", false, 0), ("done", true, 0)]
        );
        assert_eq!(rows[1].task.id, second.id);
        assert_eq!(rows[2].parent_title.as_deref(), Some("top"));
        assert!(tasks.subtasks(&second.id).unwrap().is_empty());
    }

    #[test]
    fn a_task_that_is_parent_and_subtask_has_both_markers() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let top = tasks.create("top", 4, &[]).unwrap();
        let mid = subtask(&tasks, &top, "mid");
        subtask(&tasks, &mid, "leaf");
        let rows = tasks.rows().unwrap();
        let mid = &rows[1];
        assert_eq!(mid.parent_title.as_deref(), Some("top"));
        assert_eq!(mid.open_subtasks, 1);
    }

    #[test]
    fn closed_subtasks_are_not_counted() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let done = subtask(&tasks, &parent, "done");
        let rejected = subtask(&tasks, &parent, "rejected");
        subtask(&tasks, &parent, "open");
        tasks.close(&done.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        tasks.close(&rejected.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        let rows = tasks.rows().unwrap();
        assert_eq!(rows[0].open_subtasks, 1);
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_pending_done_subtask_stays_listed_but_is_no_longer_counted() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        tasks.close(&child.id, Closing::Done).unwrap();
        let rows = tasks.rows().unwrap();
        assert!(rows[1].task.done);
        assert_eq!(rows[0].open_subtasks, 0);
    }

    #[test]
    fn tasks_live_on_any_files() {
        let files = Arc::new(crate::files::MemoryFiles::default());
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        let task = tasks.create("Buy milk", 4, &[]).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        assert_eq!(known(&tasks, "h"), ["home"]);
        let listed = list_of(&tasks);
        assert_eq!(
            (listed[0].title.as_str(), listed[0].tags.clone()),
            ("Buy milk", vec!["home".to_owned()])
        );
        let changed = files.take_changes();
        assert_eq!(changed.len(), 2, "{changed:?}"); // the Task and the Tags
        assert!(changed.contains(&PathBuf::from("Tags/tags.json")));
        assert!(changed.contains(&PathBuf::from(format!("Tasks/{}.md", task.id))));
    }
}
