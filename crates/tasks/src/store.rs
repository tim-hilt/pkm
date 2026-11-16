//! The Tasks store: a folder of `<id>.md` files.

mod filters;

pub use filters::Filter;

use crate::cache::Cache;
use crate::files::{DiskFiles, Files};
use crate::history::{Event, History};
use crate::query::QueryError;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::{Document, Value};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use pkm_base::tag;
use pkm_base::tags::{Suggestions, Tags};
use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const DEFAULT_PRIORITY: u8 = 4;
const TAGS: &str = "tags";
const BLOCKED_BY: &str = "blocked-by";
const POSITION: &str = "position";

/// Where an Open Task is planned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Planning {
    Active,
    Backlog,
}

impl Planning {
    fn state(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Backlog => "backlog",
        }
    }

    /// What the History records for a Task that joins this list.
    fn event(self) -> Event {
        match self {
            Self::Active => Event::MadeActive,
            Self::Backlog => Event::MovedToBacklog,
        }
    }
}

/// How a Task is Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Done,
    Rejected,
}

impl Closing {
    pub(crate) fn state(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Rejected => "rejected",
        }
    }

    pub(crate) fn parse(state: &str) -> Option<Self> {
        match state {
            "done" => Some(Self::Done),
            "rejected" => Some(Self::Rejected),
            _ => None,
        }
    }

    fn event(self) -> Event {
        match self {
            Self::Done => Event::Done,
            Self::Rejected => Event::Rejected,
        }
    }
}

/// A Close not yet saved, which `revert` can still take back.
#[derive(Debug, Clone)]
struct Pending {
    /// The Task and its Open Subtasks, which are Closed with it.
    ids: Vec<ItemId>,
    closing: Closing,
    /// When the user Closed it, which the History records.
    at_ms: f64,
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
    pub planning: Planning,
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

/// A Task as one row of a flat list, with what its markers show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub task: Task,
    /// The Title of its Parent (whose ID is `task.parent`), if it is a Subtask.
    pub parent_title: Option<String>,
    /// How many of its direct Subtasks, Open or Done, are Done.
    pub subtasks_done: u32,
    /// How many direct Subtasks it has, Open or Done; Rejected ones don't count.
    pub subtasks_total: u32,
}

#[derive(Debug)]
pub enum Error {
    /// The Title is empty or only whitespace.
    EmptyTitle,
    /// A Tag with a forbidden character, or empty, or a misplaced `/`.
    InvalidTag,
    /// A Subtask follows its top-level Task's Active or Backlog.
    SubtaskNotPlannable,
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
    /// A Subtask only moves among its siblings: the Task to move isn't a
    /// Subtask, or the Task to move before isn't its sibling.
    NotASibling,
    Io(io::Error),
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
            Self::SubtaskNotPlannable => {
                f.write_str("A Subtask follows its Task's Active or Backlog.")
            }
            Self::BlockedByItself => f.write_str("A Task can't be Blocked by itself."),
            Self::BlockedByParent => f.write_str("A Task can't be Blocked by its Parent."),
            Self::BlockedBySubtask => f.write_str("A Task can't be Blocked by its Subtask."),
            Self::BlockedByLoop => f.write_str("Tasks can't be Blocked by each other in a loop."),
            Self::NotASibling => f.write_str("A Subtask moves only among its siblings."),
            Self::EmptyFilterTitle => f.write_str("A Filter needs a Title."),
            Self::InvalidQuery(e) => e.fmt(f),
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
    history: History,
    /// What the files last said, so a launch reads only those that changed.
    cache: Mutex<Cache>,
    /// The one Close that can still be reverted.
    pending: Mutex<Option<Pending>>,
    /// Held while the Tags file is read and written back.
    tags_lock: Mutex<()>,
}

impl Tasks {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Tasks";

    /// Opens the Tasks in `root`, creating the folder if needed. `device` is
    /// this Device's ID, which names its History files in `root/.history`.
    /// `cache` is a file local to the Device and never Synced (ADR-0021): it
    /// remembers the Closed Tasks between launches. Missing or unreadable, it
    /// is rebuilt from the Task files.
    pub fn open(
        root: impl Into<PathBuf>,
        device: ItemId,
        cache: impl Into<PathBuf>,
    ) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self::open_in(
            Arc::new(DiskFiles),
            root,
            device,
            Some(cache.into()),
        ))
    }

    /// The Tasks in `root` on any `files`, which must hold the folder `root`
    /// (an empty one is none). Without a `cache` file, nothing is remembered
    /// between launches.
    pub fn open_in(
        files: Arc<dyn Files>,
        root: impl Into<PathBuf>,
        device: ItemId,
        cache: Option<PathBuf>,
    ) -> Self {
        let root = root.into();
        Self {
            history: History::new(files.clone(), root.join(".history"), device),
            cache: Mutex::new(Cache::new(files.clone(), cache)),
            files,
            root,
            pending: Mutex::new(None),
            tags_lock: Mutex::new(()),
        }
    }

    /// Creates a Task at the end of its list, with `priority` (clamped to 1 to
    /// 4) and `tags`, in one write with one `created` History entry; Tags that
    /// are new join the Tags file. The Title is trimmed and kept to one line
    /// (line breaks become spaces). Fails with `EmptyTitle`, or `InvalidTag`
    /// for any invalid Tag, before saving anything.
    pub fn create(
        &self,
        title: &str,
        planning: Planning,
        priority: u8,
        tags: &[String],
    ) -> Result<Task, Error> {
        let new = NewTask::checked(title, priority, tags)?;
        let position = end_position(&list(&self.load()?, planning));
        self.insert(&new, planning, None, position)
    }

    /// `create` for a Subtask directly below the Open Task `parent`, after its
    /// other Subtasks. It follows its top-level Task's Active or Backlog. Fails with `NotFound`
    /// if there is no such Open Task.
    pub fn create_subtask(
        &self,
        parent: &str,
        title: &str,
        priority: u8,
        tags: &[String],
    ) -> Result<Task, Error> {
        let new = NewTask::checked(title, priority, tags)?;
        let nodes = self.load()?;
        let parent = nodes
            .get(parent)
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?;
        let planning = parent.task.planning;
        // After the Parent's last Subtask at any depth, so the list keeps the tree order.
        let subtree = open_subtree(&nodes, &parent.task.id);
        let last = subtree
            .last()
            .map_or(parent.task.id.as_str(), ItemId::as_str);
        let list = list(&nodes, planning);
        let placement = place(&positions(&list), index_below(&list, last));
        self.shift(&list, &placement.shifted)?;
        self.insert(&new, planning, Some(&parent.task.id), placement.position)
    }

    /// Saves a new Task under a fresh Item ID and records its creation.
    fn insert(
        &self,
        new: &NewTask,
        planning: Planning,
        parent: Option<&str>,
        position: f64,
    ) -> Result<Task, Error> {
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", &new.title);
            file.set_scalar("priority", new.priority.to_string());
            file.set_scalar("state", planning.state());
            if let Some(parent) = parent {
                file.set_scalar("parent", parent);
            }
            set_list(&mut file, TAGS, new.tags.clone());
            file.set_scalar(POSITION, position.to_string());
            file.set_scalar("updated", iso8601(now_ms()));
            match self.write(&id, &file, Overwrite::No) {
                Ok(()) => {
                    self.history.record(Event::Created, &id, now_ms())?;
                    return Ok(task_of(&file).ok_or(io::ErrorKind::InvalidData)?);
                }
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

    /// Plans the Open top-level Task `id` as `planning`, at the end of that
    /// list, with its Open Subtasks, and records it in the History for each. A
    /// Task already planned so stays untouched. Fails with `NotFound` if there
    /// is no such Task and `SubtaskNotPlannable` for a Subtask.
    pub fn plan(&self, id: &str, planning: Planning) -> Result<Task, Error> {
        let nodes = self.load()?;
        let task = &nodes
            .get(id)
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?
            .task;
        if task.parent.is_some() {
            return Err(Error::SubtaskNotPlannable);
        }
        if task.planning == planning {
            return Ok(task.clone());
        }
        let list = list(&nodes, planning);
        let end = end_position(&list);
        // The Task, then its Open Subtasks in their relative order, one after
        // the other at the end of the list.
        let mut placed = None;
        for (n, item) in open_subtree(&nodes, id).into_iter().enumerate() {
            let position = end + n as f64;
            let file = self.rewrite(&item, |file| {
                file.set_scalar("state", planning.state());
                file.set_scalar(POSITION, position.to_string());
                if n == 0 {
                    file.remove("parent"); // a Parent Sync left missing or Closed
                }
            })?;
            self.history.record(planning.event(), &item, now_ms())?;
            if n == 0 {
                placed = Some(file);
            }
        }
        Ok(placed
            .as_ref()
            .and_then(task_of)
            .ok_or(io::ErrorKind::InvalidData)?)
    }

    /// Moves the Open Task `id` before the Task `before` in its list, or to
    /// the end of it if that is `None`; `before` is the Task itself: nothing
    /// changes. Its Parent stays, whoever it is. Only the moved Task is
    /// rewritten, plus, where neighbours share a key and so leave no room
    /// between them, the Tasks that share it from the drop on (see `place`).
    /// Fails with `NotFound` for an unknown or Closed Task, or a `before` that
    /// isn't in its list.
    pub fn move_before(&self, id: &str, before: Option<&str>) -> Result<Task, Error> {
        let nodes = self.load()?;
        let moved = nodes
            .get(id)
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?;
        if before == Some(id) {
            return Ok(moved.task.clone());
        }
        let mut list = list(&nodes, moved.task.planning);
        list.retain(|n| n.task.id != id);
        if before.is_some_and(|before| !list.iter().any(|n| n.task.id == before)) {
            return Err(io::ErrorKind::NotFound.into());
        }
        self.place_moved(id, &list, index_before(&list, before))
    }

    /// Puts the moved Task `id` at index `at` of `list` (without it): writes
    /// its `position` and those of the Tasks `place` shifted, then returns it.
    fn place_moved(&self, id: &str, list: &[&Node], at: usize) -> Result<Task, Error> {
        let placement = place(&positions(list), at);
        self.shift(list, &placement.shifted)?;
        let position = placement.position;
        Ok(self
            .rewrite(&ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?, |file| {
                file.set_scalar(POSITION, position.to_string());
            })
            .map(|file| task_of(&file))?
            .ok_or(io::ErrorKind::InvalidData)?)
    }

    /// Rewrites the `position` of each Task of `list` that `shifted` names by
    /// its index.
    fn shift(&self, list: &[&Node], shifted: &[(usize, f64)]) -> Result<(), Error> {
        for &(at, position) in shifted {
            let id = ItemId::parse(&list[at].task.id).ok_or(io::ErrorKind::InvalidData)?;
            self.rewrite(&id, |file| file.set_scalar(POSITION, position.to_string()))?;
        }
        Ok(())
    }

    /// Moves the Open Subtask `id` before its sibling `before`, or after its
    /// last sibling if that is `None`; `before` is the Subtask itself: nothing
    /// changes. Writes one `position`, never the Parent. At the end it lands
    /// below the last sibling's own Subtasks, so the list keeps the tree
    /// order. Fails with `NotFound` for an unknown or Closed Task and
    /// `NotASibling` for a top-level Task or a `before` that isn't a sibling,
    /// before saving anything.
    pub fn move_subtask(&self, id: &str, before: Option<&str>) -> Result<Task, Error> {
        let nodes = self.load()?;
        let moved = nodes
            .get(id)
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?;
        let parent = moved.task.parent.as_deref().ok_or(Error::NotASibling)?;
        if before == Some(id) {
            return Ok(moved.task.clone());
        }
        let mut list = list(&nodes, moved.task.planning);
        list.retain(|n| n.task.id != id);
        let siblings: Vec<&Node> = (list.iter().copied())
            .filter(|n| n.task.parent.as_deref() == Some(parent))
            .collect();
        let at = match before {
            Some(before) if siblings.iter().any(|n| n.task.id == before) => {
                index_before(&list, Some(before))
            }
            Some(_) => return Err(Error::NotASibling),
            None => {
                let Some(last) = siblings.last() else {
                    return Ok(moved.task.clone());
                };
                let tail = open_subtree(&nodes, &last.task.id);
                let end = tail.last().map_or(last.task.id.as_str(), ItemId::as_str);
                index_below(&list, end)
            }
        };
        self.place_moved(id, &list, at)
    }

    /// Closes the Open Task `id` and its Open Subtasks as one pending change:
    /// nothing is saved until `commit`, and `revert` leaves no trace. A Close
    /// still pending is committed first. There is no way to Delete a Task.
    /// Fails with `NotFound` if there is no such Open Task.
    pub fn close(&self, id: &str, closing: Closing) -> Result<(), Error> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        self.commit()?;
        let nodes = self.load()?;
        nodes
            .get(id.as_str())
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?;
        *self.pending() = Some(Pending {
            ids: open_subtree(&nodes, id.as_str()),
            closing,
            at_ms: now_ms(),
        });
        Ok(())
    }

    /// Drops the pending Close, if any, as if it never happened.
    pub fn revert(&self) {
        self.pending().take();
    }

    /// Saves the pending Close, if any: sets the `state` of the Task and its
    /// Subtasks and appends `done` or `rejected` to the History for each. One
    /// Closed or removed meanwhile, e.g. by Sync, is left as it is.
    pub fn commit(&self) -> Result<(), Error> {
        let Some(Pending {
            ids,
            closing,
            at_ms,
        }) = self.pending().take()
        else {
            return Ok(());
        };
        for id in ids {
            match self.rewrite(&id, |file| file.set_scalar("state", closing.state())) {
                Ok(_) => self.history.record(closing.event(), &id, at_ms)?,
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
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
        let open = |id: &str| nodes.get(id).filter(|n| !n.is_closed());
        open(id).ok_or(io::ErrorKind::NotFound)?;
        open(blocker).ok_or(io::ErrorKind::NotFound)?;
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

    /// The Open Tasks of both lists, as rows, that the Open Task `id` could be
    /// Blocked by: those `add_blocker` accepts, minus the ones it is Blocked by
    /// already. Fails with `NotFound` if there is no such Task.
    pub fn blocker_candidates(&self, id: &str) -> Result<Vec<Row>, Error> {
        let nodes = self.load()?;
        let task = nodes
            .get(id)
            .filter(|n| !n.is_closed())
            .ok_or(io::ErrorKind::NotFound)?;
        let mut rows = self.rows(Planning::Active)?;
        rows.extend(self.rows(Planning::Backlog)?);
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

    /// Removes `tag` from every Task, Open or Closed, saving each changed file
    /// with a new `updated`. The History and the Tags are left alone; repeating
    /// it after an interruption finishes the job.
    pub fn remove_tag_everywhere(&self, tag: &str) -> io::Result<()> {
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

    /// Forgets `tag` and its Tag Color. Call `remove_tag_everywhere` first, or
    /// a Task still carrying it brings it back, grey.
    pub fn forget_tag(&self, tag: &str) -> io::Result<()> {
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

    /// The Open Tasks planned as `planning`, Subtasks included, as one flat
    /// list in their manual order. A pending Done shows marked, a pending
    /// Rejected not at all. Structure Sync left broken is repaired here, not in
    /// the files. The counts of Done Subtasks come from the cache, which keeps
    /// Closed Tasks without their files.
    pub fn rows(&self, planning: Planning) -> io::Result<Vec<Row>> {
        let nodes = self.shown()?;
        Ok(rows_of(&nodes, list(&nodes, planning)))
    }

    /// The direct Subtasks of the Open Task `parent`, in either list, as
    /// `rows` shows them: Open or pending Done, never Rejected, in their order.
    /// Empty for a Task that has none or isn't there.
    pub fn subtasks(&self, parent: &str) -> io::Result<Vec<Row>> {
        let nodes = self.shown()?;
        let mut found: Vec<&Node> = (nodes.values())
            .filter(|n| !n.is_closed() && n.task.parent.as_deref() == Some(parent))
            .collect();
        found.sort_by(|a, b| by_position(a, b));
        Ok(rows_of(&nodes, found))
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
    /// copy under another name (`<id> 1.md`) would shadow it. Reads only the
    /// files that changed since the cache last saw them.
    fn load(&self) -> io::Result<Nodes> {
        let read_tags = RefCell::new(BTreeSet::new());
        let mut nodes = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .nodes(&self.root, |path| {
                let (node, tags) = read_node(self.files.as_ref(), path)?;
                read_tags.borrow_mut().extend(tags);
                Some(node)
            })?;
        self.remember_tags(read_tags.into_inner())?;
        repair(&mut nodes);
        for (id, position) in derive_positions(&mut nodes) {
            // Not a change of the user's: `updated` stays, so that Devices
            // migrating at once write the same bytes.
            let (mut file, _) = self.read_open(&id).map_err(|e| match e {
                Error::Io(e) => e,
                other => io::Error::other(other.to_string()),
            })?;
            file.set_scalar(POSITION, position.to_string());
            file.remove("order");
            self.write(&id, &file, Overwrite::Yes)?;
        }
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
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .forget(&format!("{id}.md"));
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
    /// How it was Closed; `None` while it is Open.
    pub(crate) closing: Option<Closing>,
    /// The place in its Active or Backlog list; see `derive_positions`.
    pub(crate) position: f64,
    /// For an Open Task whose file has no `position` yet (it dates from before
    /// that key): its legacy `order`.
    unplaced: Option<f64>,
    /// The IDs the file lists, whether or not such a Task exists.
    blocked_by: Vec<String>,
}

impl Node {
    pub(crate) fn is_closed(&self) -> bool {
        self.closing.is_some()
    }

    /// A Closed Task as the cache keeps it: what Open Tasks and lists need of
    /// it (how it was Closed, its place, Title and Parent), nothing more.
    pub(crate) fn closed(
        id: String,
        closing: Closing,
        title: String,
        parent: Option<String>,
        position: f64,
    ) -> Self {
        let task = Task {
            id,
            title,
            priority: DEFAULT_PRIORITY,
            planning: Planning::Active, // set by `repair`
            description: String::new(),
            tags: Vec::new(),
            done: false,
            parent,
            blocked: false,
            blocked_by: Vec::new(),
        };
        Self {
            task,
            closing: Some(closing),
            position,
            unplaced: None,
            blocked_by: Vec::new(),
        }
    }
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

/// The Task a file describes, Open or Closed; `None` for a malformed one.
fn node_of(file: &Document) -> Option<Node> {
    let (closing, planning) = match file.scalar("state")? {
        "active" => (None, Planning::Active),
        "backlog" => (None, Planning::Backlog),
        "done" => (Some(Closing::Done), Planning::Active), // planning set by `repair`
        "rejected" => (Some(Closing::Rejected), Planning::Active),
        _ => return None,
    };
    let task = Task {
        id: file.scalar("id")?.to_owned(),
        title: file.scalar("title")?.to_owned(),
        priority: file
            .scalar("priority")
            .and_then(|p| p.parse().ok())
            .filter(|p| (1..=4).contains(p))
            .unwrap_or(DEFAULT_PRIORITY),
        planning,
        description: file.body.trim().to_owned(),
        tags: tags_of(file),
        done: false,
        parent: file
            .scalar("parent")
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
        blocked: false,
        blocked_by: Vec::new(),
    };
    let position: Option<f64> = file.scalar(POSITION).and_then(|p| p.parse().ok());
    let position_or_zero = position.unwrap_or(0.0);
    if let Some(closing) = closing {
        return Some(Node::closed(
            task.id,
            closing,
            task.title,
            task.parent,
            position_or_zero,
        ));
    }
    let legacy_order = file.scalar("order").and_then(|o| o.parse().ok());
    Some(Node {
        task,
        closing,
        position: position_or_zero,
        unplaced: position.is_none().then(|| legacy_order.unwrap_or(0.0)),
        blocked_by: blockers_of(file),
    })
}

/// Whether `file` is a Task, Open or Closed.
pub(crate) fn is_task(file: &Document) -> bool {
    node_of(file).is_some()
}

/// The Open Task a file describes; `None` for a closed or malformed one.
fn task_of(file: &Document) -> Option<Task> {
    node_of(file).filter(|n| !n.is_closed()).map(|n| n.task)
}

/// Fixes what no Device allowed but Sync can leave (ADR-0021): an Open Task
/// whose Parent is missing or Closed becomes top-level, a Parent cycle is cut
/// at the smallest Item ID, and a Subtask is planned as its top-level Task.
fn repair(nodes: &mut Nodes) {
    let orphans: Vec<String> = nodes
        .values()
        .filter(|n| {
            n.task.parent.as_ref().is_some_and(|p| {
                nodes
                    .get(p)
                    .is_none_or(|parent| parent.is_closed() && !n.is_closed())
            })
        })
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

    let planned: Vec<(String, Planning)> = nodes
        .keys()
        .map(|id| {
            let mut root = &nodes[id];
            while let Some(parent) = &root.task.parent {
                root = &nodes[parent];
            }
            (id.clone(), root.task.planning)
        })
        .collect();
    for (id, planning) in planned {
        nodes.entry(id).and_modify(|n| n.task.planning = planning);
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
                closed: n.is_closed() || n.task.done,
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

/// Why the Open Task `id` can't be Blocked by the Open Task `blocker`, if so.
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

/// `root`, then its Open Subtasks at any depth in their list order.
fn open_subtree(nodes: &Nodes, root: &str) -> Vec<ItemId> {
    let mut ids = vec![root.to_owned()];
    let mut next = 0;
    while next < ids.len() {
        let parent = ids[next].clone();
        next += 1;
        ids.extend(
            (nodes.values())
                .filter(|n| !n.is_closed() && n.task.parent.as_deref() == Some(&parent))
                .map(|n| n.task.id.clone()),
        );
    }
    ids[1..].sort_by(|a, b| by_position(&nodes[a], &nodes[b]));
    ids.iter().filter_map(|id| ItemId::parse(id)).collect()
}

/// `found` as Rows, with the markers counted over all `nodes`.
fn rows_of(nodes: &Nodes, found: Vec<&Node>) -> Vec<Row> {
    let counts = subtask_counts(nodes);
    found
        .into_iter()
        .map(|node| {
            let task = &node.task;
            let (subtasks_done, subtasks_total) =
                counts.get(task.id.as_str()).copied().unwrap_or_default();
            Row {
                task: task.clone(),
                parent_title: (task.parent.as_ref())
                    .and_then(|p| nodes.get(p))
                    .map(|p| p.task.title.clone()),
                subtasks_done,
                subtasks_total,
            }
        })
        .collect()
}

/// The Open Tasks planned as `planning`, Subtasks included, in their order.
fn list(nodes: &Nodes, planning: Planning) -> Vec<&Node> {
    let mut found: Vec<&Node> = nodes
        .values()
        .filter(|n| !n.is_closed() && n.task.planning == planning)
        .collect();
    found.sort_by(|a, b| by_position(a, b));
    found
}

/// The `position` that puts a Task after the last Task of `list`.
fn end_position(list: &[&Node]) -> f64 {
    end_key(&positions(list))
}

/// The key after the last of `keys`.
fn end_key(keys: &[f64]) -> f64 {
    keys.last().map_or(1.0, |k| k + 1.0)
}

fn positions(list: &[&Node]) -> Vec<f64> {
    list.iter().map(|n| n.position).collect()
}

/// Where in `list` a Task goes to land right before `before`, or at the end
/// if that is `None` or not in `list`.
fn index_before(list: &[&Node], before: Option<&str>) -> usize {
    before
        .and_then(|b| list.iter().position(|n| n.task.id == b))
        .unwrap_or(list.len())
}

/// Where in `list` a Task goes to land right below the Task `above`.
fn index_below(list: &[&Node], above: &str) -> usize {
    list.iter()
        .position(|n| n.task.id == above)
        .map_or(0, |at| at + 1)
}

/// A new fractional key for an entry, and the entries that had to move for
/// it, each by its index with its new key.
struct Placement {
    position: f64,
    shifted: Vec<(usize, f64)>,
}

/// The `Placement` that puts an entry at index `at` of the ascending `keys`
/// of a list (without the entry): of Tasks in Active or the Backlog, of
/// Filters, or of the Tasks in one Filter. Usually only the entry gets a key,
/// between its neighbours'. Where they share one there is no key between
/// them, so the entries of that run from `at` on move up, spread below the
/// next key, ahead of which the entry goes.
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

/// Gives every Open Task still without a `position` one, as the migration from
/// `order` (ADR-0021): in today's tree order (top-level Tasks by `order`, each
/// followed by its Subtasks, depth first), after the Tasks that have one. It
/// is deterministic, so Devices migrating at once agree. Returns the Tasks it
/// changed with their `position`.
fn derive_positions(nodes: &mut Nodes) -> Vec<(ItemId, f64)> {
    if nodes.values().all(|n| n.unplaced.is_none()) {
        return Vec::new();
    }
    let key = |n: &Node| n.unplaced.unwrap_or(n.position);
    let mut derived = Vec::new();
    for planning in [Planning::Active, Planning::Backlog] {
        let open: Vec<&Node> = nodes
            .values()
            .filter(|n| !n.is_closed() && n.task.planning == planning)
            .collect();
        let mut last = (open.iter())
            .filter(|n| n.unplaced.is_none())
            .fold(0.0, |last, n| f64::max(last, n.position));
        let mut children: HashMap<Option<&str>, Vec<&Node>> = HashMap::new();
        for node in open {
            children
                .entry(node.task.parent.as_deref())
                .or_default()
                .push(node);
        }
        for siblings in children.values_mut() {
            siblings.sort_by(|a, b| {
                (key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal))
                    .then_with(|| a.task.id.cmp(&b.task.id))
            });
        }
        let mut todo = children.remove(&None).unwrap_or_default();
        todo.reverse();
        while let Some(node) = todo.pop() {
            if node.unplaced.is_some() {
                last += 1.0;
                derived.push((node.task.id.clone(), last));
            }
            if let Some(subtasks) = children.remove(&Some(node.task.id.as_str())) {
                todo.extend(subtasks.into_iter().rev());
            }
        }
    }
    derived
        .into_iter()
        .filter_map(|(id, position)| {
            let node = nodes.get_mut(&id)?;
            node.position = position;
            node.unplaced = None;
            Some((ItemId::parse(&id)?, position))
        })
        .collect()
}

fn by_position(a: &Node, b: &Node) -> Ordering {
    compare_positions(a.position, b.position).then_with(|| a.task.id.cmp(&b.task.id))
}

/// Fractional keys in order; one that is no number ties.
fn compare_positions(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// What a Parent's marker shows, by Parent ID: its direct Subtasks that are
/// Done, and all of them but the Rejected.
fn subtask_counts(nodes: &Nodes) -> HashMap<&str, (u32, u32)> {
    let mut counts: HashMap<&str, (u32, u32)> = HashMap::new();
    for node in nodes.values() {
        let Some(parent) = node.task.parent.as_deref() else {
            continue;
        };
        if node.closing == Some(Closing::Rejected) {
            continue;
        }
        let count = counts.entry(parent).or_default();
        count.0 += u32::from(node.closing == Some(Closing::Done) || node.task.done);
        count.1 += 1;
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
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let mut entropy = Entropy::from_system();
            let dir = std::env::temp_dir().join(format!("pkm-tasks-{}", ItemId::new(&mut entropy)));
            Self(dir.join("Tasks"))
        }

        fn store(&self) -> Tasks {
            Tasks::open(
                &self.0,
                ItemId::parse("dddddddddddd").unwrap(),
                self.cache(),
            )
            .unwrap()
        }

        fn tags_file(&self) -> PathBuf {
            self.0.parent().unwrap().join(Tags::FILE)
        }

        /// Beside the Tasks folder, as a Device keeps it.
        fn cache(&self) -> PathBuf {
            self.0.parent().unwrap().join("cache")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(self.0.parent().unwrap()));
        }
    }

    /// The Tasks of a list, as `rows` lists them.
    fn list_of(tasks: &Tasks, planning: Planning) -> Vec<Task> {
        let rows = tasks.rows(planning).unwrap();
        rows.into_iter().map(|r| r.task).collect()
    }

    fn titles(tasks: &Tasks, planning: Planning) -> Vec<String> {
        list_of(tasks, planning)
            .into_iter()
            .map(|t| t.title)
            .collect()
    }

    #[test]
    fn a_new_task_is_listed_where_it_was_planned() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks.create("Buy milk", Planning::Active, 4, &[]).unwrap();
        tasks
            .create("Learn Rust", Planning::Backlog, 4, &[])
            .unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["Buy milk"]);
        assert_eq!(titles(&tasks, Planning::Backlog), ["Learn Rust"]);
    }

    #[test]
    fn tasks_are_listed_in_the_order_they_were_created() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["one", "two", "three"] {
            tasks.create(title, Planning::Active, 4, &[]).unwrap();
        }
        assert_eq!(titles(&tasks, Planning::Active), ["one", "two", "three"]);
    }

    #[test]
    fn a_new_task_has_priority_4() {
        let dir = Scratch::new();
        let task = dir.store().create("x", Planning::Active, 4, &[]).unwrap();
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
        let task = tasks
            .create("Fix login", Planning::Active, 1, &tags)
            .unwrap();
        assert_eq!(
            (task.priority, task.tags.clone()),
            (1, tag_names(&["bug", "work"]))
        );
        assert_eq!(
            list_of(&tasks, Planning::Active),
            std::slice::from_ref(&task)
        );
        let text = file_of(&dir, &task);
        assert!(
            text.contains("priority: 1") && text.contains("tags:"),
            "{text}"
        );
        assert_eq!(history(&dir).len(), 1);
    }

    #[test]
    fn the_tags_of_a_new_task_join_the_tags_file() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks
            .create("a", Planning::Active, 4, &tag_names(&["new", "work"]))
            .unwrap();
        let known: Vec<_> = tasks.tags().list().into_iter().map(|e| e.name).collect();
        assert_eq!(known, ["new", "work"]);
    }

    #[test]
    fn an_invalid_tag_or_an_empty_title_creates_nothing() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let bad = tag_names(&["ok", "Bad"]);
        assert!(matches!(
            tasks.create("a", Planning::Active, 4, &bad),
            Err(Error::InvalidTag)
        ));
        assert!(matches!(
            tasks.create(" ", Planning::Active, 1, &[]),
            Err(Error::EmptyTitle)
        ));
        assert!(titles(&tasks, Planning::Active).is_empty());
        assert!(!dir.0.join(".history").exists());
        assert!(tasks.tags().list().is_empty());
    }

    #[test]
    fn a_priority_outside_1_to_4_is_clamped_on_create() {
        let dir = Scratch::new();
        let tasks = dir.store();
        assert_eq!(
            tasks
                .create("a", Planning::Active, 0, &[])
                .unwrap()
                .priority,
            1
        );
        assert_eq!(
            tasks
                .create("b", Planning::Active, 9, &[])
                .unwrap()
                .priority,
            4
        );
    }

    #[test]
    fn a_subtask_is_created_with_its_priority_and_tags() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog, 4, &[]).unwrap();
        let sub = tasks
            .create_subtask(&parent.id, "s", 2, &tag_names(&["home"]))
            .unwrap();
        assert_eq!((sub.priority, sub.tags), (2, tag_names(&["home"])));
        assert_eq!(history(&dir).len(), 2);
        assert!(tasks.tags().contains("home"));
    }

    #[test]
    fn an_empty_or_blank_title_is_refused() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["", "   ", "\n\t \n"] {
            assert!(matches!(
                tasks.create(title, Planning::Active, 4, &[]),
                Err(Error::EmptyTitle)
            ));
        }
        assert!(titles(&tasks, Planning::Active).is_empty());
    }

    #[test]
    fn the_title_is_trimmed_and_kept_to_one_line() {
        let dir = Scratch::new();
        let task = dir
            .store()
            .create("  Buy\n  milk \r\n\n and eggs ", Planning::Active, 4, &[])
            .unwrap();
        assert_eq!(task.title, "Buy milk and eggs");
    }

    #[test]
    fn a_task_is_a_markdown_file_named_by_its_id_with_front_matter() {
        let dir = Scratch::new();
        let task = dir
            .store()
            .create("Buy: milk", Planning::Backlog, 4, &[])
            .unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        let file = Document::parse(&text).unwrap();
        assert_eq!(file.scalar("id"), Some(task.id.as_str()));
        assert_eq!(file.scalar("title"), Some("Buy: milk"));
        assert_eq!(file.scalar("priority"), Some("4"));
        assert_eq!(file.scalar("state"), Some("backlog"));
        assert!(file.scalar("position").is_some());
        assert!(file.scalar("order").is_none());
        let updated = file.scalar("updated").unwrap();
        assert!(updated.len() == 20 && updated.ends_with('Z'), "{updated}");
    }

    #[test]
    fn tasks_persist_across_opens() {
        let dir = Scratch::new();
        dir.store()
            .create("kept", Planning::Active, 4, &[])
            .unwrap();
        assert_eq!(titles(&dir.store(), Planning::Active), ["kept"]);
    }

    fn edit_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        let task = tasks.create("Buy milk", Planning::Active, 4, &[]).unwrap();
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
        assert_eq!(list_of(&tasks, Planning::Active), [edited]);
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
        assert_eq!(list_of(&tasks, Planning::Active), [task]);
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
        assert_eq!(list_of(&tasks, Planning::Active), [task]);
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
        assert_eq!(file.scalar("state"), Some("active"));
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
        assert!(titles(&tasks, Planning::Active).is_empty());
    }

    /// The Tags suggested for `query`, leaving none out.
    fn known(tasks: &Tasks, query: &str) -> Vec<String> {
        tasks.suggest_tags(query, &[]).unwrap().matches
    }

    fn tagged(tasks: &Tasks, tags: &[&str]) -> Task {
        let task = tasks.create("t", Planning::Active, 4, &[]).unwrap();
        for tag in tags {
            tasks.add_tag(&task.id, tag).unwrap();
        }
        list_of(tasks, Planning::Active).pop().unwrap()
    }

    #[test]
    fn tags_are_added_in_order_and_stored_as_a_block_list() {
        let dir = Scratch::new();
        let task = tagged(&dir.store(), &["home", "work/x"]);
        assert_eq!(task.tags, ["home", "work/x"]);
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.contains("tags:\n  - home\n  - work/x\n"), "{text}");
        assert_eq!(list_of(&dir.store(), Planning::Active), [task]);
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
        assert_eq!(list_of(&tasks, Planning::Active), [task]);
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
        fs::write(&path, text.replace("state:", "tags:\n  - hand\nstate:")).unwrap();
        assert!(known(&tasks, "").is_empty()); // not read yet
        tasks.edit(&task.id, "new", "", 4, &[]).unwrap();
        assert_eq!(known(&tasks, ""), ["hand"]);
    }

    #[test]
    fn reading_a_task_file_with_an_unknown_tag_adds_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", Planning::Active, 4, &[]).unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path).unwrap();
        let text = text.replace("state:", "tags:\n  - synced\n  - Bad Tag\nstate:");
        fs::write(&path, text).unwrap();
        list_of(&tasks, Planning::Active);
        assert_eq!(known(&tasks, ""), ["synced"]);
    }

    #[test]
    fn removing_a_tag_everywhere_reaches_closed_tasks_and_leaves_the_history() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let open = tagged(&tasks, &["home", "work"]);
        let closed = tagged(&tasks, &["work"]);
        let other = tagged(&tasks, &["home"]);
        tasks.close(&closed.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        let path = |t: &Task| dir.0.join(format!("{}.md", t.id));
        let stamp = |t: &Task| {
            let mut file = Document::parse(&fs::read_to_string(path(t)).unwrap()).unwrap();
            file.set_scalar("updated", "2000-01-01T00:00:00Z");
            fs::write(path(t), file.render()).unwrap();
        };
        for task in [&open, &closed, &other] {
            stamp(task);
        }
        let other_text = fs::read_to_string(path(&other)).unwrap();
        let history_before = history(&dir);

        tasks.remove_tag_everywhere("work").unwrap();

        for task in [&open, &closed] {
            let file = Document::parse(&fs::read_to_string(path(task)).unwrap()).unwrap();
            assert!(!tags_of(&file).contains(&"work".to_owned()));
            assert_ne!(file.scalar("updated"), Some("2000-01-01T00:00:00Z"));
        }
        assert_eq!(
            tags_of(&Document::parse(&fs::read_to_string(path(&open)).unwrap()).unwrap()),
            ["home"]
        );
        assert_eq!(fs::read_to_string(path(&other)).unwrap(), other_text);
        assert_eq!(history(&dir), history_before);
    }

    #[test]
    fn a_forgotten_tag_is_gone_and_starts_grey_when_added_again() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["work"]);
        tasks.set_tag_color("work", Some("#d0021b")).unwrap();
        tasks.remove_tag_everywhere("work").unwrap();
        tasks.forget_tag("work").unwrap();
        assert!(!tasks.tags().contains("work"));
        tasks.add_tag(&task.id, "work").unwrap();
        assert!(tasks.tags().contains("work"));
        assert_eq!(tasks.tags().color("work"), None);
    }

    fn history(dir: &Scratch) -> Vec<String> {
        let mut lines = Vec::new();
        for entry in fs::read_dir(dir.0.join(".history")).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            assert!(
                name.starts_with("dddddddddddd-") && name.ends_with(".log"),
                "{name}"
            );
            lines.extend(fs::read_to_string(path).unwrap().lines().map(String::from));
        }
        lines
    }

    #[test]
    fn a_backlog_task_can_be_made_active_and_moved_back() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks.create("first", Planning::Active, 4, &[]).unwrap();
        let later = tasks.create("later", Planning::Backlog, 4, &[]).unwrap();
        let moved = tasks.plan(&later.id, Planning::Active).unwrap();
        assert_eq!(moved.planning, Planning::Active);
        assert_eq!(titles(&tasks, Planning::Active), ["first", "later"]);
        assert!(titles(&tasks, Planning::Backlog).is_empty());
        tasks.plan(&later.id, Planning::Backlog).unwrap();
        assert_eq!(titles(&tasks, Planning::Backlog), ["later"]);
    }

    #[test]
    fn planning_an_unknown_task_fails() {
        let dir = Scratch::new();
        assert!(dir.store().plan("aaaaaaaaaaaa", Planning::Active).is_err());
    }

    #[test]
    fn planning_keeps_the_other_properties() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", Planning::Backlog, 4, &[]).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        let moved = tasks.plan(&task.id, Planning::Active).unwrap();
        assert_eq!(moved.tags, ["home"]);
    }

    #[test]
    fn creating_and_planning_are_recorded_by_id_and_time_only() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks
            .create("Secret title", Planning::Backlog, 4, &[])
            .unwrap();
        tasks.add_tag(&task.id, "private").unwrap();
        tasks.plan(&task.id, Planning::Active).unwrap();
        tasks.plan(&task.id, Planning::Active).unwrap(); // no change, no line
        tasks.plan(&task.id, Planning::Backlog).unwrap();
        let lines = history(&dir);
        let events: Vec<_> = lines
            .iter()
            .map(|l| {
                let parts: Vec<_> = l.split(' ').collect();
                assert_eq!(parts.len(), 3, "{l}");
                assert!(parts[0].ends_with('Z'));
                assert_eq!(parts[2], task.id);
                parts[1]
            })
            .collect();
        assert_eq!(events, ["created", "made-active", "moved-to-backlog"]);
    }

    fn closing_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        tasks.create("other", Planning::Active, 4, &[]).unwrap();
        let task = tasks.create("Buy milk", Planning::Active, 4, &[]).unwrap();
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
        let listed = list_of(&tasks, Planning::Active);
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
        assert_eq!(titles(&tasks, Planning::Active), ["other"]);
    }

    #[test]
    fn reverting_leaves_no_trace_in_files_or_history() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        let (before, lines) = (file_of(&dir, &task), history(&dir));
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.revert();
        tasks.commit().unwrap();
        assert_eq!(list_of(&tasks, Planning::Active).len(), 2);
        assert!(list_of(&tasks, Planning::Active).iter().all(|t| !t.done));
        assert_eq!((file_of(&dir, &task), history(&dir)), (before, lines));
    }

    #[test]
    fn nothing_is_saved_before_the_commit() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Done).unwrap();
        assert!(file_of(&dir, &task).contains("state: active"));
        assert_eq!(history(&dir).len(), 2); // the two creations
    }

    #[test]
    fn committing_saves_the_state_and_appends_to_the_history() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(file_of(&dir, &task).contains("state: done"));
        assert_eq!(titles(&dir.store(), Planning::Active), ["other"]);
        assert!(
            history(&dir)
                .last()
                .unwrap()
                .ends_with(&format!(" done {}", task.id))
        );
        tasks.commit().unwrap(); // nothing pending: no second line
        assert_eq!(history(&dir).len(), 3);
    }

    #[test]
    fn a_rejected_task_is_recorded_as_rejected() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        assert!(file_of(&dir, &task).contains("state: rejected"));
        assert!(
            history(&dir)
                .last()
                .unwrap()
                .ends_with(&format!(" rejected {}", task.id))
        );
    }

    #[test]
    fn a_new_close_commits_the_previous_one_first() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let first = tasks.create("first", Planning::Active, 4, &[]).unwrap();
        let second = tasks.create("second", Planning::Active, 4, &[]).unwrap();
        tasks.close(&first.id, Closing::Done).unwrap();
        tasks.close(&second.id, Closing::Rejected).unwrap();
        assert!(file_of(&dir, &first).contains("state: done"));
        assert!(file_of(&dir, &second).contains("state: active"));
        tasks.revert();
        assert_eq!(titles(&tasks, Planning::Active), ["second"]);
    }

    #[test]
    fn closing_an_unknown_task_fails() {
        let dir = Scratch::new();
        assert!(dir.store().close("aaaaaaaaaaaa", Closing::Done).is_err());
    }

    #[test]
    fn a_pending_close_keeps_edits_made_meanwhile() {
        let dir = Scratch::new();
        let (tasks, task) = closing_fixture(&dir);
        tasks.close(&task.id, Closing::Done).unwrap();
        tasks.edit(&task.id, "Renamed", "", 1, &[]).unwrap();
        tasks.commit().unwrap();
        let text = file_of(&dir, &task);
        assert!(
            text.contains("title: Renamed") && text.contains("state: done"),
            "{text}"
        );
    }

    fn subtask(tasks: &Tasks, parent: &Task, title: &str) -> Task {
        tasks.create_subtask(&parent.id, title, 4, &[]).unwrap()
    }

    /// Each row's Title with its Parent's, in list order.
    fn tree(tasks: &Tasks, planning: Planning) -> Vec<(String, Option<String>)> {
        let rows = tasks.rows(planning).unwrap();
        rows.into_iter()
            .map(|r| (r.task.title, r.parent_title))
            .collect()
    }

    /// `(title, parent)` as `tree` lists them.
    fn sub(title: &str, parent: Option<&str>) -> (String, Option<String>) {
        (title.to_owned(), parent.map(str::to_owned))
    }

    /// A raw Task file, as Sync or another tool could leave it.
    fn raw(dir: &Scratch, id: &str, state: &str, parent: Option<&str>, order: u8) {
        let parent = parent.map_or(String::new(), |p| format!("parent: {p}\n"));
        let text = format!(
            "---\nid: {id}\ntitle: {id}\npriority: 4\nstate: {state}\n{parent}order: {order}\nupdated: 2000-01-01T00:00:00Z\n---\n"
        );
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join(format!("{id}.md")), text).unwrap();
    }

    #[test]
    fn a_subtask_is_listed_under_its_parent_in_the_parents_list() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("parent", Planning::Backlog, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "child");
        let grandchild = subtask(&tasks, &child, "grandchild");
        subtask(&tasks, &parent, "second");
        assert_eq!(child.planning, Planning::Backlog);
        assert_eq!(grandchild.parent.as_deref(), Some(child.id.as_str()));
        assert!(titles(&tasks, Planning::Active).is_empty());
        assert_eq!(
            tree(&tasks, Planning::Backlog),
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
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        assert!(matches!(
            tasks.create_subtask(&parent.id, " ", 4, &[]),
            Err(Error::EmptyTitle)
        ));
        assert!(matches!(
            tasks.create_subtask("aaaaaaaaaaaa", "x", 4, &[]),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
        raw(&dir, "bbbbbbbbbbbb", "done", None, 1);
        assert!(tasks.create_subtask("bbbbbbbbbbbb", "x", 4, &[]).is_err());
    }

    #[test]
    fn planning_a_task_rewrites_its_open_subtasks_and_records_each() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        let grandchild = subtask(&tasks, &child, "g");
        raw(&dir, "cccccccccccc", "done", Some(&parent.id), 9);
        let before = history(&dir).len();
        tasks.plan(&parent.id, Planning::Active).unwrap();
        assert!(file_of(&dir, &child).contains("state: active"));
        assert!(file_of(&dir, &grandchild).contains("state: active"));
        let lines = history(&dir);
        let made_active: Vec<_> = lines[before..]
            .iter()
            .map(|l| l.rsplit(' ').collect::<Vec<_>>())
            .collect();
        assert_eq!(made_active.len(), 3);
        for id in [&parent.id, &child.id, &grandchild.id] {
            assert!(
                made_active
                    .iter()
                    .any(|l| l[0] == id && l[1] == "made-active")
            );
        }
        assert_eq!(
            fs::read_to_string(dir.0.join("cccccccccccc.md"))
                .unwrap()
                .matches("state: done")
                .count(),
            1
        );
        assert_eq!(
            tree(&tasks, Planning::Active),
            [sub("p", None), sub("c", Some("p")), sub("g", Some("c"))]
        );
    }

    #[test]
    fn a_subtask_cannot_be_made_active_or_moved_by_itself() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        for planning in [Planning::Active, Planning::Backlog] {
            assert!(matches!(
                tasks.plan(&child.id, planning),
                Err(Error::SubtaskNotPlannable)
            ));
        }
        assert!(file_of(&dir, &child).contains("state: backlog"));
    }

    #[test]
    fn closing_a_parent_closes_its_open_subtasks_in_one_pending_change() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        let grandchild = subtask(&tasks, &child, "g");
        let before = (file_of(&dir, &child), history(&dir));
        tasks.close(&parent.id, Closing::Done).unwrap();
        assert!(list_of(&tasks, Planning::Active).iter().all(|t| t.done));
        tasks.revert();
        tasks.commit().unwrap();
        assert_eq!((file_of(&dir, &child), history(&dir)), before);

        tasks.close(&parent.id, Closing::Rejected).unwrap();
        assert!(list_of(&tasks, Planning::Active).is_empty());
        tasks.commit().unwrap();
        for task in [&parent, &child, &grandchild] {
            assert!(file_of(&dir, task).contains("state: rejected"));
            assert!(
                history(&dir)
                    .iter()
                    .any(|l| l.ends_with(&format!(" rejected {}", task.id)))
            );
        }
    }

    #[test]
    fn a_closed_subtask_leaves_the_list_once_the_close_is_committed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        let open = subtask(&tasks, &parent, "open");
        let gone = subtask(&tasks, &parent, "gone");
        tasks.close(&gone.id, Closing::Rejected).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["p", "open"]);
        tasks.commit().unwrap();
        assert_eq!(list_of(&tasks, Planning::Active)[1].id, open.id);
        assert_eq!(titles(&tasks, Planning::Active), ["p", "open"]);
        tasks.close(&parent.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(list_of(&tasks, Planning::Active).is_empty());
    }

    #[test]
    fn an_orphan_becomes_top_level() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "aaaaaaaaaaaa", "backlog", Some("zzzzzzzzzzzz"), 1);
        assert_eq!(titles(&tasks, Planning::Backlog), ["aaaaaaaaaaaa"]);
        tasks.plan("aaaaaaaaaaaa", Planning::Active).unwrap();
        assert!(
            !fs::read_to_string(dir.0.join("aaaaaaaaaaaa.md"))
                .unwrap()
                .contains("parent")
        );
    }

    #[test]
    fn a_parent_cycle_is_cut_at_the_smallest_item_id() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "bbbbbbbbbbbb", "active", Some("cccccccccccc"), 1);
        raw(&dir, "cccccccccccc", "active", Some("aaaaaaaaaaaa"), 1);
        raw(&dir, "aaaaaaaaaaaa", "active", Some("bbbbbbbbbbbb"), 1);
        raw(&dir, "dddddddddddd", "active", Some("dddddddddddd"), 1);
        let parents: HashMap<_, _> = tree(&tasks, Planning::Active).into_iter().collect();
        let parent = |id: &str| parents[id].clone();
        assert_eq!(parent("aaaaaaaaaaaa"), None);
        assert_eq!(parent("dddddddddddd"), None);
        assert_eq!(parent("cccccccccccc").as_deref(), Some("aaaaaaaaaaaa"));
        assert_eq!(parent("bbbbbbbbbbbb").as_deref(), Some("cccccccccccc"));
    }

    #[test]
    fn a_subtask_is_planned_as_its_top_level_task() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "aaaaaaaaaaaa", "backlog", None, 1);
        raw(&dir, "bbbbbbbbbbbb", "active", Some("aaaaaaaaaaaa"), 1);
        raw(&dir, "cccccccccccc", "active", Some("bbbbbbbbbbbb"), 1);
        assert!(titles(&tasks, Planning::Active).is_empty());
        assert_eq!(titles(&tasks, Planning::Backlog).len(), 3);
    }

    #[test]
    fn a_copy_of_a_task_file_under_another_name_is_ignored() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("original", Planning::Active, 4, &[]).unwrap();
        let copy = file_of(&dir, &task).replace("original", "copy");
        for name in [format!("{} 1.md", task.id), format!("~{}.md", task.id)] {
            fs::write(dir.0.join(name), &copy).unwrap();
        }
        tasks.edit(&task.id, "edited", "", 4, &[]).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["edited"]);
    }

    #[test]
    fn committing_skips_a_subtask_closed_or_removed_meanwhile() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        let gone = subtask(&tasks, &parent, "gone");
        let kept = subtask(&tasks, &parent, "kept");
        tasks.close(&parent.id, Closing::Done).unwrap();
        fs::remove_file(dir.0.join(format!("{}.md", gone.id))).unwrap();
        tasks.commit().unwrap();
        assert!(file_of(&dir, &parent).contains("state: done"));
        assert!(file_of(&dir, &kept).contains("state: done"));
        assert!(titles(&tasks, Planning::Active).is_empty());
    }

    #[test]
    fn an_open_subtask_of_a_closed_parent_becomes_top_level() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "aaaaaaaaaaaa", "done", None, 1);
        raw(&dir, "bbbbbbbbbbbb", "active", Some("aaaaaaaaaaaa"), 1);
        assert_eq!(titles(&tasks, Planning::Active), ["bbbbbbbbbbbb"]);
    }

    fn move_fixture(dir: &Scratch) -> (Tasks, [Task; 3]) {
        let tasks = dir.store();
        let [a, b, c] = ["a", "b", "c"].map(|t| tasks.create(t, Planning::Active, 4, &[]).unwrap());
        (tasks, [a, b, c])
    }

    #[test]
    fn a_task_moves_before_a_sibling_or_to_the_end() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        tasks.move_before(&c.id, Some(&a.id)).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["c", "a", "b"]);
        tasks.move_before(&b.id, Some(&a.id)).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["c", "b", "a"]);
        tasks.move_before(&c.id, None).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["b", "a", "c"]);
        assert_eq!(titles(&dir.store(), Planning::Active), ["b", "a", "c"]);
    }

    #[test]
    fn a_move_within_a_list_rewrites_only_the_moved_task() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        let before = (file_of(&dir, &a), file_of(&dir, &b), history(&dir));
        tasks.move_before(&c.id, Some(&b.id)).unwrap();
        assert_eq!(
            (file_of(&dir, &a), file_of(&dir, &b), history(&dir)),
            before
        );
    }

    #[test]
    fn equal_keys_sort_by_item_id() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "cccccccccccc", "active", None, 1);
        raw(&dir, "aaaaaaaaaaaa", "active", None, 1);
        raw(&dir, "bbbbbbbbbbbb", "active", None, 1);
        assert_eq!(
            titles(&tasks, Planning::Active),
            ["aaaaaaaaaaaa", "bbbbbbbbbbbb", "cccccccccccc"]
        );
    }

    #[test]
    fn a_move_between_tasks_sharing_a_position_lands_where_dropped() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let [a, b, c, d] = [
            "aaaaaaaaaaaa",
            "bbbbbbbbbbbb",
            "cccccccccccc",
            "dddddddddddd",
        ];
        for id in [a, b, c, d] {
            raw_positioned(&dir, id, 1.0);
        }
        raw_positioned(&dir, "eeeeeeeeeeee", 2.0);
        tasks.move_before(d, Some(b)).unwrap();
        assert_eq!(
            titles(&tasks, Planning::Active),
            [a, d, b, c, "eeeeeeeeeeee"]
        );
        // Down into the middle of the tie, as the jump back showed on iPhone.
        tasks.move_before(a, Some(c)).unwrap();
        assert_eq!(
            titles(&dir.store(), Planning::Active),
            [d, b, a, c, "eeeeeeeeeeee"]
        );
    }

    #[test]
    fn a_move_into_a_tie_rewrites_only_the_tasks_after_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let ids = [
            "aaaaaaaaaaaa",
            "bbbbbbbbbbbb",
            "cccccccccccc",
            "dddddddddddd",
        ];
        for id in ids {
            raw_positioned(&dir, id, 1.0);
        }
        let untouched = file_of(&dir, &tasks.rows(Planning::Active).unwrap()[0].task);
        tasks.move_before(ids[3], Some(ids[2])).unwrap();
        assert_eq!(
            file_of(&dir, &tasks.rows(Planning::Active).unwrap()[0].task),
            untouched
        );
    }

    #[test]
    fn a_subtask_moved_to_the_end_lands_below_a_tie() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (p, q) = ("aaaaaaaaaaaa", "bbbbbbbbbbbb");
        raw_positioned(&dir, p, 1.0);
        raw_positioned(&dir, q, 1.0);
        raw_positioned(&dir, "cccccccccccc", 1.0);
        let one = tasks.create_subtask(p, "one", 4, &[]).unwrap();
        tasks.create_subtask(p, "two", 4, &[]).unwrap();
        assert_eq!(
            titles(&tasks, Planning::Active),
            [p, "one", "two", q, "cccccccccccc"]
        );
        tasks.move_subtask(&one.id, None).unwrap();
        assert_eq!(
            titles(&dir.store(), Planning::Active),
            [p, "two", "one", q, "cccccccccccc"]
        );
    }

    #[test]
    fn a_subtask_moves_above_its_parent_and_keeps_it() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let sub = subtask(&tasks, &a, "sub");
        let moved = tasks.move_before(&sub.id, Some(&a.id)).unwrap();
        assert_eq!(moved.parent.as_deref(), Some(a.id.as_str()));
        let rows = tasks.rows(Planning::Active).unwrap();
        let listed: Vec<_> = rows
            .iter()
            .map(|r| (&*r.task.title, r.parent_title.as_deref()))
            .collect();
        assert_eq!(
            listed,
            [("sub", Some("a")), ("a", None), ("b", None), ("c", None)]
        );
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
        assert_eq!(
            titles(&tasks, Planning::Active),
            ["a", "x", "y", "z", "b", "c"]
        );
    }

    #[test]
    fn a_subtask_moved_to_the_end_lands_below_the_last_siblings_subtasks() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let x = subtask(&tasks, &a, "x");
        let y = subtask(&tasks, &a, "y");
        subtask(&tasks, &y, "y1");
        tasks.move_subtask(&x.id, None).unwrap();
        assert_eq!(
            titles(&tasks, Planning::Active),
            ["a", "y", "y1", "x", "b", "c"]
        );
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

    #[test]
    fn a_task_moves_among_another_tasks_subtasks_without_joining_it() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        subtask(&tasks, &a, "sub");
        tasks.move_before(&c.id, Some(&b.id)).unwrap();
        tasks.move_before(&b.id, None).unwrap();
        let rows = tasks.rows(Planning::Active).unwrap();
        assert_eq!(
            rows.iter().map(|r| &*r.task.title).collect::<Vec<_>>(),
            ["a", "sub", "c", "b"]
        );
        assert!(rows[2].task.parent.is_none() && rows[3].task.parent.is_none());
    }

    #[test]
    fn moving_before_itself_changes_nothing() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let before = file_of(&dir, &a);
        tasks.move_before(&a.id, Some(&a.id)).unwrap();
        assert_eq!(file_of(&dir, &a), before);
    }

    #[test]
    fn moving_an_unknown_task_or_before_a_task_outside_its_list_fails() {
        let dir = Scratch::new();
        let (tasks, [_, b, _]) = move_fixture(&dir);
        let backlog = tasks.create("x", Planning::Backlog, 4, &[]).unwrap();
        raw(&dir, "dddddddddddd", "done", None, 1);
        for (id, before) in [
            ("aaaaaaaaaaaa", None),
            ("dddddddddddd", None),
            (&b.id[..], Some("aaaaaaaaaaaa")),
            (&b.id[..], Some("dddddddddddd")),
            (&b.id[..], Some(&backlog.id[..])),
        ] {
            assert!(
                matches!(
                    tasks.move_before(id, before),
                    Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
                ),
                "{id} {before:?}"
            );
        }
    }

    /// A raw Task file with a `position` and no `order`.
    fn raw_positioned(dir: &Scratch, id: &str, position: f64) {
        let text = format!(
            "---\nid: {id}\ntitle: {id}\npriority: 4\nstate: active\nposition: {position}\nupdated: 2000-01-01T00:00:00Z\n---\n"
        );
        fs::create_dir_all(&dir.0).unwrap();
        fs::write(dir.0.join(format!("{id}.md")), text).unwrap();
    }

    fn files_in(dir: &Scratch) -> Vec<(String, String)> {
        let mut files: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().ends_with(".md"))
            .map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                (name, fs::read_to_string(e.path()).unwrap())
            })
            .collect();
        files.sort();
        files
    }

    #[test]
    fn files_with_only_an_order_list_in_the_tree_order_and_then_carry_a_position() {
        let dir = Scratch::new();
        raw(&dir, "aaaaaaaaaaaa", "active", None, 2);
        raw(&dir, "bbbbbbbbbbbb", "active", None, 1);
        raw(&dir, "cccccccccccc", "active", None, 1); // ties with b: by Item ID
        raw(&dir, "dddddddddddd", "active", Some("bbbbbbbbbbbb"), 2);
        raw(&dir, "eeeeeeeeeeee", "active", Some("bbbbbbbbbbbb"), 1);
        raw(&dir, "ffffffffffff", "active", Some("eeeeeeeeeeee"), 1);
        raw(&dir, "gggggggggggg", "backlog", None, 5);
        let expected = ["bbbb", "eeee", "ffff", "dddd", "cccc", "aaaa"].map(|p| p.repeat(3));
        let rows = |tasks: &Tasks| -> Vec<String> {
            let rows = tasks.rows(Planning::Active).unwrap();
            rows.into_iter().map(|r| r.task.id).collect()
        };
        assert_eq!(rows(&dir.store()), expected);

        let migrated = files_in(&dir);
        for (name, text) in &migrated {
            let file = Document::parse(text).unwrap();
            assert!(file.scalar("position").is_some(), "{name}");
            assert!(file.scalar("order").is_none(), "{name}");
        }
        // Once: opening again rewrites nothing and lists the same.
        assert_eq!(rows(&dir.store()), expected);
        assert_eq!(files_in(&dir), migrated);
    }

    #[test]
    fn migrating_keeps_the_updated_time_of_every_file() {
        let dir = Scratch::new();
        raw(&dir, "aaaaaaaaaaaa", "active", None, 1);
        list_of(&dir.store(), Planning::Active);
        assert!(
            files_in(&dir)[0]
                .1
                .contains("updated: 2000-01-01T00:00:00Z")
        );
    }

    #[test]
    fn a_folder_with_migrated_and_unmigrated_files_lists_the_unmigrated_last() {
        let dir = Scratch::new();
        raw_positioned(&dir, "pppppppppppp", 7.5);
        raw_positioned(&dir, "qqqqqqqqqqqq", -3.0);
        raw(&dir, "zzzzzzzzzzzz", "active", None, 1);
        raw(&dir, "yyyyyyyyyyyy", "active", Some("zzzzzzzzzzzz"), 1);
        let before = files_in(&dir);
        let tasks = dir.store();
        assert_eq!(
            titles(&tasks, Planning::Active),
            [
                "qqqqqqqqqqqq",
                "pppppppppppp",
                "zzzzzzzzzzzz",
                "yyyyyyyyyyyy"
            ]
        );
        for (name, text) in files_in(&dir) {
            let unchanged = before.iter().any(|(n, t)| *n == name && *t == text);
            assert_eq!(unchanged, name.starts_with(['p', 'q']), "{name}");
        }
    }

    #[test]
    fn a_move_writes_a_fractional_position_and_nothing_else_changes() {
        let dir = Scratch::new();
        let (tasks, [_, b, c]) = move_fixture(&dir);
        tasks.move_before(&c.id, Some(&b.id)).unwrap();
        let text = file_of(&dir, &c);
        let position: f64 = Document::parse(&text)
            .unwrap()
            .scalar("position")
            .unwrap()
            .parse()
            .unwrap();
        assert!(1.0 < position && position < 2.0, "{position}");
        assert!(!text.contains("order"));
        assert_eq!(titles(&tasks, Planning::Active), ["a", "c", "b"]);
    }

    #[test]
    fn a_subtask_lands_directly_below_its_parent_and_a_task_at_the_end() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        tasks.create("next", Planning::Active, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "child");
        subtask(&tasks, &child, "grandchild");
        tasks.create("last", Planning::Active, 4, &[]).unwrap();
        let rows: Vec<_> = tasks
            .rows(Planning::Active)
            .unwrap()
            .into_iter()
            .map(|r| r.task.title)
            .collect();
        assert_eq!(rows, ["p", "child", "grandchild", "next", "last"]);
    }

    #[test]
    fn planning_a_task_appends_it_then_its_subtasks_in_their_order() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog, 4, &[]).unwrap();
        let first = subtask(&tasks, &parent, "first");
        let second = subtask(&tasks, &parent, "second");
        subtask(&tasks, &first, "nested");
        tasks.move_before(&second.id, Some(&parent.id)).unwrap(); // second, p, first, nested
        tasks.create("other", Planning::Active, 4, &[]).unwrap();
        tasks.plan(&parent.id, Planning::Active).unwrap();
        let rows: Vec<_> = tasks
            .rows(Planning::Active)
            .unwrap()
            .into_iter()
            .map(|r| r.task.title)
            .collect();
        assert_eq!(rows, ["other", "p", "second", "first", "nested"]);
    }

    fn two_tasks(tasks: &Tasks) -> (Task, Task) {
        (
            tasks.create("a", Planning::Active, 4, &[]).unwrap(),
            tasks.create("b", Planning::Backlog, 4, &[]).unwrap(),
        )
    }

    fn listed(tasks: &Tasks, id: &str) -> Task {
        [Planning::Active, Planning::Backlog]
            .into_iter()
            .flat_map(|p| list_of(tasks, p))
            .find(|t| t.id == id)
            .unwrap()
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
        let c = tasks.create("c", Planning::Active, 4, &[]).unwrap();
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
        assert_eq!(a.blocked_by.len(), 2);
        assert!(a.blocked_by.iter().all(|b| b.closed));
    }

    #[test]
    fn a_blocked_task_can_still_be_closed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, b) = two_tasks(&tasks);
        tasks.add_blocker(&a.id, &b.id).unwrap();
        tasks.close(&a.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(file_of(&dir, &a).contains("state: done"));
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
        let c = tasks.create("c", Planning::Active, 4, &[]).unwrap();
        tasks.add_blocker(&b.id, &c.id).unwrap();
        assert!(matches!(
            tasks.add_blocker(&c.id, &a.id),
            Err(Error::BlockedByLoop)
        ));
        assert_eq!(file_of(&dir, &a), before);
    }

    #[test]
    fn blocking_by_an_unknown_or_closed_task_fails() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let (a, _) = two_tasks(&tasks);
        raw(&dir, "dddddddddddd", "done", None, 1);
        for blocker in ["aaaaaaaaaaaa", "dddddddddddd"] {
            assert!(matches!(
                tasks.add_blocker(&a.id, blocker),
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
            ));
        }
    }

    #[test]
    fn blocker_candidates_leave_out_what_add_blocker_refuses() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let a = tasks.create("a", Planning::Active, 4, &[]).unwrap();
        let sub = tasks.create_subtask(&a.id, "sub", 4, &[]).unwrap();
        let b = tasks.create("b", Planning::Backlog, 4, &[]).unwrap();
        let c = tasks.create("c", Planning::Active, 4, &[]).unwrap();
        let d = tasks.create("d", Planning::Backlog, 4, &[]).unwrap();
        let e = tasks.create("e", Planning::Active, 4, &[]).unwrap();
        let gone = tasks.create("gone", Planning::Active, 4, &[]).unwrap();
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
        assert_eq!(titles(&sub.id), ["c", "e", "b", "d"]); // not a, its Parent
        assert_eq!(titles(&d.id), ["a", "sub", "c", "e", "b"]);
        assert!(matches!(
            tasks.blocker_candidates(&gone.id),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    /// `raw` with a `blocked-by` list.
    fn raw_blocked(dir: &Scratch, id: &str, state: &str, blockers: &[&str]) {
        raw(dir, id, state, None, 1);
        let path = dir.0.join(format!("{id}.md"));
        let list: String = blockers.iter().map(|b| format!("  - {b}\n")).collect();
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("order: 1\n", &format!("order: 1\nblocked-by:\n{list}"));
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
        let listed = list_of(&tasks, Planning::Active);
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|t| t.blocked));
        let d = tasks.create("d", Planning::Active, 4, &[]).unwrap();
        tasks.add_blocker(&d.id, "aaaaaaaaaaaa").unwrap();
        assert!(matches!(
            tasks.add_blocker("bbbbbbbbbbbb", &d.id),
            Err(Error::BlockedByLoop)
        ));
    }

    /// Makes every Task file unreadable, so a listing that still works read
    /// none of them. The modification date stays.
    #[cfg(unix)]
    fn lock_files(dir: &Scratch) {
        for item in fs::read_dir(&dir.0).unwrap() {
            let path = item.unwrap().path();
            if path.extension().is_some_and(|e| e == "md") {
                lock_file(&path);
            }
        }
    }

    #[cfg(unix)]
    fn lock_file(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o000)).unwrap();
    }

    fn cache_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        let parent = tasks.create("parent", Planning::Active, 4, &[]).unwrap();
        let gone = subtask(&tasks, &parent, "gone");
        subtask(&tasks, &parent, "open");
        tasks.close(&gone.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        (tasks, gone)
    }

    #[cfg(unix)]
    #[test]
    fn listing_an_unchanged_folder_reads_no_task_file() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        let before = list_of(&tasks, Planning::Active);
        lock_files(&dir);
        assert_eq!(list_of(&tasks, Planning::Active), before);
    }

    #[test]
    fn the_cache_keeps_the_position_of_a_closed_task() {
        let dir = Scratch::new();
        let (tasks, gone) = cache_fixture(&dir);
        list_of(&tasks, Planning::Active);
        let position = Document::parse(&file_of(&dir, &gone))
            .unwrap()
            .scalar("position")
            .unwrap()
            .to_owned();
        let cached = fs::read_to_string(dir.cache()).unwrap();
        assert!(
            cached.contains(&format!("\t{position}\tgone\n")),
            "{cached}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn closed_tasks_are_remembered_across_launches() {
        let dir = Scratch::new();
        let (tasks, gone) = cache_fixture(&dir);
        list_of(&tasks, Planning::Active);
        lock_file(&dir.0.join(format!("{}.md", gone.id)));
        let rows = dir.store().rows(Planning::Active).unwrap();
        assert_eq!(
            (rows[0].subtasks_done, rows[0].subtasks_total),
            (1, 2),
            "{}",
            gone.id
        );
    }

    #[test]
    fn a_file_changed_added_or_removed_outside_the_app_shows_up() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let kept = tasks.create("kept", Planning::Active, 4, &[]).unwrap();
        let other = tasks.create("other", Planning::Active, 4, &[]).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["kept", "other"]);

        let path = dir.0.join(format!("{}.md", kept.id));
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("kept", "changed");
        fs::write(&path, text).unwrap();
        raw(&dir, "aaaaaaaaaaaa", "active", None, 0);
        fs::remove_file(dir.0.join(format!("{}.md", other.id))).unwrap();
        // A Task found without a position joins the end.
        assert_eq!(
            titles(&tasks, Planning::Active),
            ["changed", "aaaaaaaaaaaa"]
        );
        assert_eq!(
            titles(&dir.store(), Planning::Active),
            ["changed", "aaaaaaaaaaaa"]
        );
    }

    #[test]
    fn a_task_closed_by_another_device_is_no_longer_open() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", Planning::Active, 4, &[]).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["t"]);
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("state: active", "state: rejected");
        fs::write(&path, text).unwrap();
        assert!(titles(&tasks, Planning::Active).is_empty());
    }

    #[test]
    fn a_garbled_cache_is_rebuilt_from_the_files() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        list_of(&tasks, Planning::Active);
        fs::write(dir.cache(), "nonsense\n\t\t\n").unwrap();
        assert_eq!(list_of(&dir.store(), Planning::Active).len(), 2);
    }

    #[test]
    fn the_cache_is_kept_outside_the_synced_folder() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        list_of(&tasks, Planning::Active);
        assert!(dir.cache().exists());
        let names: Vec<_> = fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert!(
            names.iter().all(|n| n.ends_with(".md") || n == ".history"),
            "{names:?}"
        );
    }

    #[test]
    fn the_cache_file_is_rewritten_only_when_a_closed_task_changes() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        let parent = list_of(&tasks, Planning::Active).remove(0);
        fs::remove_file(dir.cache()).unwrap();
        tasks.edit(&parent.id, "renamed", "", 1, &[]).unwrap();
        tasks.create("new", Planning::Active, 4, &[]).unwrap();
        list_of(&tasks, Planning::Active);
        assert!(!dir.cache().exists());
        tasks.close(&parent.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        list_of(&tasks, Planning::Active);
        assert!(dir.cache().exists());
    }

    #[test]
    fn rows_list_each_task_before_its_subtasks() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("parent", Planning::Active, 4, &[]).unwrap();
        tasks.create_subtask(&parent.id, "child", 4, &[]).unwrap();
        tasks.create("other", Planning::Active, 4, &[]).unwrap();
        let rows: Vec<_> = tasks
            .rows(Planning::Active)
            .unwrap()
            .into_iter()
            .map(|r| {
                (
                    r.task.title,
                    r.parent_title,
                    r.subtasks_done,
                    r.subtasks_total,
                )
            })
            .collect();
        let some = |t: &str| Some(t.to_owned());
        assert_eq!(
            rows,
            [
                ("parent".to_owned(), None, 0, 1),
                ("child".to_owned(), some("parent"), 0, 0),
                ("other".to_owned(), None, 0, 0)
            ]
        );
    }

    #[test]
    fn subtasks_are_the_direct_open_ones_in_order_with_list_markers() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let top = tasks.create("top", Planning::Backlog, 4, &[]).unwrap();
        let first = subtask(&tasks, &top, "first");
        let second = subtask(&tasks, &top, "second");
        subtask(&tasks, &first, "grandchild");
        let done = subtask(&tasks, &top, "done");
        let rejected = subtask(&tasks, &top, "rejected");
        tasks.create("other", Planning::Backlog, 4, &[]).unwrap();
        tasks.close(&rejected.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        tasks.close(&done.id, Closing::Done).unwrap();
        let rows = tasks.subtasks(&top.id).unwrap();
        let shown: Vec<_> = rows
            .iter()
            .map(|r| (r.task.title.as_str(), r.task.done, r.subtasks_total))
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
        let top = tasks.create("top", Planning::Active, 4, &[]).unwrap();
        let mid = subtask(&tasks, &top, "mid");
        subtask(&tasks, &mid, "leaf");
        let rows = tasks.rows(Planning::Active).unwrap();
        let mid = &rows[1];
        assert_eq!(mid.parent_title.as_deref(), Some("top"));
        assert_eq!((mid.subtasks_done, mid.subtasks_total), (0, 1));
    }

    #[test]
    fn done_subtasks_count_but_rejected_ones_do_not() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        let done = subtask(&tasks, &parent, "done");
        let rejected = subtask(&tasks, &parent, "rejected");
        subtask(&tasks, &parent, "open");
        tasks.close(&done.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        tasks.close(&rejected.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        let rows = tasks.rows(Planning::Active).unwrap();
        assert_eq!((rows[0].subtasks_done, rows[0].subtasks_total), (1, 2));
        assert_eq!(rows.len(), 2);
    }

    #[test]
    fn a_pending_done_subtask_stays_listed_and_counts_as_done() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active, 4, &[]).unwrap();
        let child = subtask(&tasks, &parent, "c");
        tasks.close(&child.id, Closing::Done).unwrap();
        let rows = tasks.rows(Planning::Active).unwrap();
        assert!(rows[1].task.done);
        assert_eq!((rows[0].subtasks_done, rows[0].subtasks_total), (1, 1));
    }

    #[cfg(unix)]
    #[test]
    fn the_counts_read_no_task_file() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        let before = tasks.rows(Planning::Active).unwrap();
        lock_files(&dir);
        assert_eq!(tasks.rows(Planning::Active).unwrap(), before);
        assert_eq!((before[0].subtasks_done, before[0].subtasks_total), (1, 2));
    }

    #[test]
    fn tasks_live_on_any_files() {
        let files = Arc::new(crate::files::MemoryFiles::default());
        let device = ItemId::parse("dddddddddddd").unwrap();
        let tasks = Tasks::open_in(files.clone(), "Tasks", device, None);
        let task = tasks.create("Buy milk", Planning::Backlog, 4, &[]).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        tasks.plan(&task.id, Planning::Active).unwrap();
        assert_eq!(known(&tasks, "h"), ["home"]);
        let listed = list_of(&tasks, Planning::Active);
        assert_eq!(
            (listed[0].title.as_str(), listed[0].tags.clone()),
            ("Buy milk", vec!["home".to_owned()])
        );
        let changed = files.take_changes();
        assert_eq!(changed.len(), 3, "{changed:?}"); // the Task, the History and the Tags
        assert!(changed.contains(&PathBuf::from("Tags/tags.json")));
        assert!(changed.contains(&PathBuf::from(format!("Tasks/{}.md", task.id))));
    }
}
