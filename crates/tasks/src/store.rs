//! The Tasks store: a folder of `<id>.md` files.

use crate::cache::Cache;
use crate::files::{DiskFiles, Files};
use crate::history::{Event, History};
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::{Document, Value};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use pkm_base::tag;
use pkm_base::tags::Tags;
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
}

/// How a Task is Closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Closing {
    Done,
    Rejected,
}

impl Closing {
    fn state(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Rejected => "rejected",
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

/// A Task as the lists show it: Open, or a Closed Subtask of an Open one.
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
    /// Already Closed: only Subtasks of an Open Task are listed so.
    pub closed: bool,
    /// The Parent's ID, if this is a Subtask.
    pub parent: Option<String>,
    /// In their order; empty for a Closed Task.
    pub subtasks: Vec<Task>,
    /// Whether any Task it's Blocked by is Open. Only listings fill this and
    /// `blocked_by`; a Task a change returns has neither.
    pub blocked: bool,
    /// The Tasks it's Blocked by, Open or Closed, in the order they were added;
    /// one that is missing is left out.
    pub blocked_by: Vec<Blocker>,
}

/// A Task as one row of a flattened list: its Subtasks follow it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// Without its `subtasks`, which are rows of their own.
    pub task: Task,
    /// How deep it sits in the tree, 0 for a top-level Task.
    pub depth: u32,
    pub has_subtasks: bool,
}

impl Row {
    fn flatten(mut task: Task, depth: u32, rows: &mut Vec<Self>) {
        let subtasks = std::mem::take(&mut task.subtasks);
        rows.push(Self {
            task,
            depth,
            has_subtasks: !subtasks.is_empty(),
        });
        for subtask in subtasks {
            Self::flatten(subtask, depth + 1, rows);
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// The Title is empty or only whitespace.
    EmptyTitle,
    /// A Tag with a forbidden character, or empty, or a misplaced `/`.
    InvalidTag,
    /// A Subtask follows its top-level Task's Active or Backlog.
    SubtaskNotPlannable,
    /// A Task can't move under itself or one of its Subtasks.
    MoveIntoOwnSubtree,
    /// A Task can't be Blocked by itself.
    BlockedByItself,
    /// A Task can't be Blocked by its Parent.
    BlockedByParent,
    /// A Task can't be Blocked by one of its Subtasks.
    BlockedBySubtask,
    /// The Blocked by would make Tasks wait on each other.
    BlockedByLoop,
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
            Self::MoveIntoOwnSubtree => {
                f.write_str("A Task can't move under itself or its Subtasks.")
            }
            Self::BlockedByItself => f.write_str("A Task can't be Blocked by itself."),
            Self::BlockedByParent => f.write_str("A Task can't be Blocked by its Parent."),
            Self::BlockedBySubtask => f.write_str("A Task can't be Blocked by its Subtask."),
            Self::BlockedByLoop => f.write_str("Tasks can't be Blocked by each other in a loop."),
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

    /// Creates a Task at the end of its list, with priority 4. The Title is
    /// trimmed and kept to one line (line breaks become spaces).
    pub fn create(&self, title: &str, planning: Planning) -> Result<Task, Error> {
        let title = checked_title(title)?;
        let order = next_order(&self.load()?, None, planning);
        self.insert(&title, planning, None, order)
    }

    /// Creates a Subtask at the end of the Open Task `parent`'s Subtasks. It
    /// follows its top-level Task's Active or Backlog. Fails with `NotFound`
    /// if there is no such Open Task.
    pub fn create_subtask(&self, parent: &str, title: &str) -> Result<Task, Error> {
        let title = checked_title(title)?;
        let nodes = self.load()?;
        let parent = nodes
            .get(parent)
            .filter(|n| !n.task.closed)
            .ok_or(io::ErrorKind::NotFound)?;
        let planning = parent.task.planning;
        let order = next_order(&nodes, Some(&parent.task.id), planning);
        self.insert(&title, planning, Some(&parent.task.id), order)
    }

    /// Saves a new Task under a fresh Item ID and records its creation.
    fn insert(
        &self,
        title: &str,
        planning: Planning,
        parent: Option<&str>,
        order: f64,
    ) -> Result<Task, Error> {
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", title);
            file.set_scalar("priority", DEFAULT_PRIORITY.to_string());
            file.set_scalar("state", planning.state());
            if let Some(parent) = parent {
                file.set_scalar("parent", parent);
            }
            file.set_scalar("order", order.to_string());
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
    /// Task `id` and bumps `updated`. A Priority outside 1 to 4 is clamped.
    /// Every other property, known or not, stays as it was. Fails with
    /// `NotFound` if there is no such Task.
    pub fn edit(
        &self,
        id: &str,
        title: &str,
        description: &str,
        priority: u8,
    ) -> Result<Task, Error> {
        let title = checked_title(title)?;
        let priority = priority.clamp(1, 4);
        let description = description.trim();
        self.modify(id, |file| {
            file.set_scalar("title", &title);
            file.set_scalar("priority", priority.to_string());
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
            .filter(|n| !n.task.closed)
            .ok_or(io::ErrorKind::NotFound)?
            .task;
        if task.parent.is_some() {
            return Err(Error::SubtaskNotPlannable);
        }
        if task.planning == planning {
            return Ok(task.clone());
        }
        let order = next_order(&nodes, None, planning);
        self.place(&nodes, id, None, planning, order)
    }

    /// Moves the Open Task `id` under the Open Task `parent`, or to the top
    /// level of the `planning` list if `parent` is `None` (`planning` is then
    /// the list it joins; under a Parent the Task follows its top-level Task's).
    /// It lands before its sibling `before`, or last if that is `None`. Only
    /// the moved Task is rewritten, unless its Active or Backlog changes: then
    /// its Open Subtasks follow, and each records it in the History. Where
    /// siblings share a key there is no room between them: the Task lands
    /// beside them, ordered by Item ID. Fails with `NotFound` for an unknown or
    /// Closed Task, Parent or `before` that is no sibling, and
    /// `MoveIntoOwnSubtree` for a Parent that is the Task or its Subtask.
    pub fn move_to(
        &self,
        id: &str,
        parent: Option<&str>,
        planning: Planning,
        before: Option<&str>,
    ) -> Result<Task, Error> {
        let nodes = self.load()?;
        let open = |id: &str| nodes.get(id).filter(|n| !n.task.closed);
        open(id).ok_or(io::ErrorKind::NotFound)?;
        let planning = match parent {
            Some(parent) => {
                let parent = open(parent).ok_or(io::ErrorKind::NotFound)?;
                if is_within(&nodes, &parent.task.id, id) {
                    return Err(Error::MoveIntoOwnSubtree);
                }
                parent.task.planning
            }
            None => planning,
        };
        let mut siblings = siblings(&nodes, parent, planning);
        siblings.retain(|n| n.task.id != id);
        let order = match before {
            None => siblings.last().map_or(1.0, |n| n.order + 1.0),
            Some(before) => {
                let at = siblings
                    .iter()
                    .position(|n| n.task.id == before)
                    .ok_or(io::ErrorKind::NotFound)?;
                match at.checked_sub(1) {
                    Some(prev) => f64::midpoint(siblings[prev].order, siblings[at].order),
                    None => siblings[at].order - 1.0,
                }
            }
        };
        self.place(&nodes, id, parent, planning, order)
    }

    /// Saves the Open Task `id` as a child of `parent` (top-level if `None`)
    /// with key `order`, planned as `planning`. If that changes its Active or
    /// Backlog its Open Subtasks follow, and each records it in the History.
    /// Returns the Task as saved.
    fn place(
        &self,
        nodes: &Nodes,
        id: &str,
        parent: Option<&str>,
        planning: Planning,
        order: f64,
    ) -> Result<Task, Error> {
        let replanned = nodes[id].task.planning != planning;
        let event = match planning {
            Planning::Active => Event::MadeActive,
            Planning::Backlog => Event::MovedToBacklog,
        };
        let mut placed = None;
        for item in open_subtree(nodes, id) {
            let moved = item.as_str() == id;
            if !moved && !replanned {
                continue;
            }
            let file = self.rewrite(&item, |file| {
                file.set_scalar("state", planning.state());
                if moved {
                    file.set_scalar("order", order.to_string());
                    match parent {
                        Some(parent) => file.set_scalar("parent", parent),
                        None => file.remove("parent"),
                    }
                }
            })?;
            if replanned {
                self.history.record(event, &item, now_ms())?;
            }
            if moved {
                placed = Some(file);
            }
        }
        Ok(placed
            .as_ref()
            .and_then(task_of)
            .ok_or(io::ErrorKind::InvalidData)?)
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
            .filter(|n| !n.task.closed)
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
        let open = |id: &str| nodes.get(id).filter(|n| !n.task.closed);
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
            .filter(|n| !n.task.closed)
            .ok_or(io::ErrorKind::NotFound)?;
        let mut rows = self.rows(Planning::Active)?;
        rows.extend(self.rows(Planning::Backlog)?);
        rows.retain(|row| {
            let candidate = &row.task;
            !candidate.closed
                && !candidate.done
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

    /// The known Tags that start with `prefix`, sorted. Reads the Tags file
    /// only, so a Tag no Task carries any more is found too.
    pub fn tags_with_prefix(&self, prefix: &str) -> io::Result<Vec<String>> {
        Ok(self.read_tags().with_prefix(prefix))
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

    /// The Open top-level Tasks planned as `planning`, in their order, each
    /// with its Subtasks. Closed Subtasks of an Open Task are included, marked
    /// closed; a pending Done shows marked, a pending Rejected not at all.
    /// Structure Sync left broken is repaired here, not in the files.
    pub fn tasks(&self, planning: Planning) -> io::Result<Vec<Task>> {
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
        let mut children: HashMap<String, Vec<Node>> = HashMap::new();
        let mut roots = Vec::new();
        for node in nodes.into_values() {
            match node.task.parent.clone() {
                Some(parent) => children.entry(parent).or_default().push(node),
                None if !node.task.closed && node.task.planning == planning => roots.push(node),
                None => {}
            }
        }
        roots.sort_by(by_order);
        Ok(roots
            .into_iter()
            .map(|root| attach(root, &mut children))
            .collect())
    }

    /// `tasks` as a pre-order walk: each Task is followed by its Subtasks.
    pub fn rows(&self, planning: Planning) -> io::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for task in self.tasks(planning)? {
            Row::flatten(task, 0, &mut rows);
        }
        Ok(rows)
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
    pub(crate) order: f64,
    /// The IDs the file lists, whether or not such a Task exists.
    blocked_by: Vec<String>,
}

impl Node {
    /// A Closed Task as the cache keeps it: what Open Tasks and lists need of
    /// it (its place, Title and Parent), nothing more.
    pub(crate) fn closed(id: String, title: String, parent: Option<String>, order: f64) -> Self {
        let task = Task {
            id,
            title,
            priority: DEFAULT_PRIORITY,
            planning: Planning::Active, // set by `repair`
            description: String::new(),
            tags: Vec::new(),
            done: false,
            closed: true,
            parent,
            subtasks: Vec::new(),
            blocked: false,
            blocked_by: Vec::new(),
        };
        Self {
            task,
            order,
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
    let (closed, planning) = match file.scalar("state")? {
        "active" => (false, Planning::Active),
        "backlog" => (false, Planning::Backlog),
        "done" | "rejected" => (true, Planning::Active), // set by `repair`
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
        closed,
        parent: file
            .scalar("parent")
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
        subtasks: Vec::new(),
        blocked: false,
        blocked_by: Vec::new(),
    };
    let order = file
        .scalar("order")
        .and_then(|o| o.parse().ok())
        .unwrap_or(0.0);
    if closed {
        return Some(Node::closed(task.id, task.title, task.parent, order));
    }
    Some(Node {
        task,
        order,
        blocked_by: blockers_of(file),
    })
}

/// Whether `file` is a Task, Open or Closed.
pub(crate) fn is_task(file: &Document) -> bool {
    node_of(file).is_some()
}

/// The Open Task a file describes; `None` for a closed or malformed one.
fn task_of(file: &Document) -> Option<Task> {
    node_of(file).map(|n| n.task).filter(|task| !task.closed)
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
                    .is_none_or(|parent| parent.task.closed && !n.task.closed)
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
                closed: n.task.closed || n.task.done,
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

/// `root` and its Open Descendants, parents before their Subtasks.
fn open_subtree(nodes: &Nodes, root: &str) -> Vec<ItemId> {
    let mut ids = vec![root.to_owned()];
    let mut next = 0;
    while next < ids.len() {
        let parent = ids[next].clone();
        next += 1;
        let mut subtasks: Vec<&Node> = nodes
            .values()
            .filter(|n| !n.task.closed && n.task.parent.as_deref() == Some(&parent))
            .collect();
        subtasks.sort_by(|a, b| by_order(a, b));
        ids.extend(subtasks.into_iter().map(|n| n.task.id.clone()));
    }
    ids.iter().filter_map(|id| ItemId::parse(id)).collect()
}

/// The Tasks sharing a list, in their order: the Subtasks of `parent` or,
/// top-level, the Open Tasks planned as `planning`.
fn siblings<'a>(nodes: &'a Nodes, parent: Option<&str>, planning: Planning) -> Vec<&'a Node> {
    let mut found: Vec<&Node> = nodes
        .values()
        .filter(|n| {
            n.task.parent.as_deref() == parent
                && (parent.is_some() || (!n.task.closed && n.task.planning == planning))
        })
        .collect();
    found.sort_by(|a, b| by_order(a, b));
    found
}

/// Where a new Task goes: after its siblings.
fn next_order(nodes: &Nodes, parent: Option<&str>, planning: Planning) -> f64 {
    siblings(nodes, parent, planning)
        .last()
        .map_or(1.0, |n| n.order + 1.0)
}

/// Whether `id` is `ancestor` or sits below it.
fn is_within(nodes: &Nodes, id: &str, ancestor: &str) -> bool {
    let mut at = Some(id);
    while let Some(current) = at {
        if current == ancestor {
            return true;
        }
        at = nodes.get(current).and_then(|n| n.task.parent.as_deref());
    }
    false
}

fn by_order(a: &Node, b: &Node) -> Ordering {
    a.order
        .partial_cmp(&b.order)
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.task.id.cmp(&b.task.id))
}

/// `node` with its Subtasks taken from `children`; a Closed Task has none.
fn attach(node: Node, children: &mut HashMap<String, Vec<Node>>) -> Task {
    let mut task = node.task;
    if !task.closed {
        let mut subtasks = children.remove(&task.id).unwrap_or_default();
        subtasks.sort_by(by_order);
        task.subtasks = subtasks.into_iter().map(|s| attach(s, children)).collect();
    }
    task
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

    fn titles(tasks: &Tasks, planning: Planning) -> Vec<String> {
        tasks
            .tasks(planning)
            .unwrap()
            .into_iter()
            .map(|t| t.title)
            .collect()
    }

    #[test]
    fn a_new_task_is_listed_where_it_was_planned() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tasks.create("Buy milk", Planning::Active).unwrap();
        tasks.create("Learn Rust", Planning::Backlog).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["Buy milk"]);
        assert_eq!(titles(&tasks, Planning::Backlog), ["Learn Rust"]);
    }

    #[test]
    fn tasks_are_listed_in_the_order_they_were_created() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["one", "two", "three"] {
            tasks.create(title, Planning::Active).unwrap();
        }
        assert_eq!(titles(&tasks, Planning::Active), ["one", "two", "three"]);
    }

    #[test]
    fn a_new_task_has_priority_4() {
        let dir = Scratch::new();
        let task = dir.store().create("x", Planning::Active).unwrap();
        assert_eq!(task.priority, 4);
    }

    #[test]
    fn an_empty_or_blank_title_is_refused() {
        let dir = Scratch::new();
        let tasks = dir.store();
        for title in ["", "   ", "\n\t \n"] {
            assert!(matches!(
                tasks.create(title, Planning::Active),
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
            .create("  Buy\n  milk \r\n\n and eggs ", Planning::Active)
            .unwrap();
        assert_eq!(task.title, "Buy milk and eggs");
    }

    #[test]
    fn a_task_is_a_markdown_file_named_by_its_id_with_front_matter() {
        let dir = Scratch::new();
        let task = dir.store().create("Buy: milk", Planning::Backlog).unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        let file = Document::parse(&text).unwrap();
        assert_eq!(file.scalar("id"), Some(task.id.as_str()));
        assert_eq!(file.scalar("title"), Some("Buy: milk"));
        assert_eq!(file.scalar("priority"), Some("4"));
        assert_eq!(file.scalar("state"), Some("backlog"));
        assert!(file.scalar("order").is_some());
        let updated = file.scalar("updated").unwrap();
        assert!(updated.len() == 20 && updated.ends_with('Z'), "{updated}");
    }

    #[test]
    fn tasks_persist_across_opens() {
        let dir = Scratch::new();
        dir.store().create("kept", Planning::Active).unwrap();
        assert_eq!(titles(&dir.store(), Planning::Active), ["kept"]);
    }

    fn edit_fixture(dir: &Scratch) -> (Tasks, Task) {
        let tasks = dir.store();
        let task = tasks.create("Buy milk", Planning::Active).unwrap();
        (tasks, task)
    }

    #[test]
    fn editing_sets_title_description_and_priority() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let edited = tasks
            .edit(&task.id, " Buy oat milk ", "Two\nlitres", 1)
            .unwrap();
        assert_eq!(edited.title, "Buy oat milk");
        assert_eq!(edited.description, "Two\nlitres");
        assert_eq!(edited.priority, 1);
        assert_eq!(tasks.tasks(Planning::Active).unwrap(), [edited]);
    }

    #[test]
    fn editing_refuses_an_empty_title_and_changes_nothing() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        assert!(matches!(
            tasks.edit(&task.id, "  ", "x", 1),
            Err(Error::EmptyTitle)
        ));
        assert_eq!(tasks.tasks(Planning::Active).unwrap(), [task]);
    }

    #[test]
    fn editing_an_unknown_task_fails() {
        let dir = Scratch::new();
        let (tasks, _) = edit_fixture(&dir);
        for id in ["abcdefgh", "../x"] {
            assert!(matches!(
                tasks.edit(id, "t", "", 4),
                Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
            ));
        }
    }

    #[test]
    fn editing_clamps_the_priority() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        assert_eq!(tasks.edit(&task.id, "t", "", 0).unwrap().priority, 1);
        assert_eq!(tasks.edit(&task.id, "t", "", 9).unwrap().priority, 4);
    }

    #[test]
    fn the_description_is_the_files_body() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        tasks.edit(&task.id, "t", "  details  ", 4).unwrap();
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.ends_with("---\ndetails\n"), "{text}");
        tasks.edit(&task.id, "t", "", 4).unwrap();
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

        tasks.edit(&task.id, "New", "body", 2).unwrap();

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
        tasks.edit(&task.id, "t", "body", 2).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let mut file = Document::parse(&fs::read_to_string(&path).unwrap()).unwrap();
        file.set_scalar("updated", "2000-01-01T00:00:00Z");
        fs::write(&path, file.render()).unwrap();
        let before = fs::read_to_string(&path).unwrap();

        tasks.edit(&task.id, " t ", "body ", 2).unwrap();
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

    fn tagged(tasks: &Tasks, tags: &[&str]) -> Task {
        let task = tasks.create("t", Planning::Active).unwrap();
        for tag in tags {
            tasks.add_tag(&task.id, tag).unwrap();
        }
        tasks.tasks(Planning::Active).unwrap().pop().unwrap()
    }

    #[test]
    fn tags_are_added_in_order_and_stored_as_a_block_list() {
        let dir = Scratch::new();
        let task = tagged(&dir.store(), &["home", "work/x"]);
        assert_eq!(task.tags, ["home", "work/x"]);
        let text = fs::read_to_string(dir.0.join(format!("{}.md", task.id))).unwrap();
        assert!(text.contains("tags:\n  - home\n  - work/x\n"), "{text}");
        assert_eq!(dir.store().tasks(Planning::Active).unwrap(), [task]);
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
        assert_eq!(tasks.tasks(Planning::Active).unwrap(), [task]);
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
        assert_eq!(tasks.edit(&task.id, "n", "d", 2).unwrap().tags, ["a"]);
        let task = tasks.add_tag(&task.id, "b").unwrap();
        assert_eq!((task.title.as_str(), task.priority), ("n", 2));
    }

    #[test]
    fn known_tags_are_listed_by_prefix_sorted_and_once() {
        let dir = Scratch::new();
        let tasks = dir.store();
        tagged(&tasks, &["work/b", "home"]);
        tagged(&tasks, &["work/a", "home", "w"]);
        assert_eq!(
            tasks.tags_with_prefix("w").unwrap(),
            ["w", "work/a", "work/b"]
        );
        assert_eq!(tasks.tags_with_prefix("").unwrap().len(), 4);
        assert!(tasks.tags_with_prefix("x").unwrap().is_empty());
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
        assert_eq!(tasks.tags_with_prefix("").unwrap(), ["gone", "kept"]);
    }

    #[test]
    fn tags_are_kept_in_the_tags_file_beside_the_context_folders() {
        let dir = Scratch::new();
        tagged(&dir.store(), &["home"]);
        let text = fs::read_to_string(dir.tags_file()).unwrap();
        assert_eq!(Tags::parse(&text).with_prefix(""), ["home"]);
    }

    #[test]
    fn saving_an_edit_adds_the_tags_the_file_carries() {
        let dir = Scratch::new();
        let (tasks, task) = edit_fixture(&dir);
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path).unwrap();
        fs::write(&path, text.replace("state:", "tags:\n  - hand\nstate:")).unwrap();
        assert!(tasks.tags_with_prefix("").unwrap().is_empty()); // not read yet
        tasks.edit(&task.id, "new", "", 4).unwrap();
        assert_eq!(tasks.tags_with_prefix("").unwrap(), ["hand"]);
    }

    #[test]
    fn reading_a_task_file_with_an_unknown_tag_adds_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", Planning::Active).unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path).unwrap();
        let text = text.replace("state:", "tags:\n  - synced\n  - Bad Tag\nstate:");
        fs::write(&path, text).unwrap();
        tasks.tasks(Planning::Active).unwrap();
        assert_eq!(tasks.tags_with_prefix("").unwrap(), ["synced"]);
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
        tasks.create("first", Planning::Active).unwrap();
        let later = tasks.create("later", Planning::Backlog).unwrap();
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
        let task = tasks.create("t", Planning::Backlog).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        let moved = tasks.plan(&task.id, Planning::Active).unwrap();
        assert_eq!(moved.tags, ["home"]);
    }

    #[test]
    fn creating_and_planning_are_recorded_by_id_and_time_only() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("Secret title", Planning::Backlog).unwrap();
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
        tasks.create("other", Planning::Active).unwrap();
        let task = tasks.create("Buy milk", Planning::Active).unwrap();
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
        let listed = tasks.tasks(Planning::Active).unwrap();
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
        assert_eq!(tasks.tasks(Planning::Active).unwrap().len(), 2);
        assert!(
            tasks
                .tasks(Planning::Active)
                .unwrap()
                .iter()
                .all(|t| !t.done)
        );
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
        let first = tasks.create("first", Planning::Active).unwrap();
        let second = tasks.create("second", Planning::Active).unwrap();
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
        tasks.edit(&task.id, "Renamed", "", 1).unwrap();
        tasks.commit().unwrap();
        let text = file_of(&dir, &task);
        assert!(
            text.contains("title: Renamed") && text.contains("state: done"),
            "{text}"
        );
    }

    fn subtask(tasks: &Tasks, parent: &Task, title: &str) -> Task {
        tasks.create_subtask(&parent.id, title).unwrap()
    }

    fn sub_titles(task: &Task) -> Vec<&str> {
        task.subtasks.iter().map(|t| t.title.as_str()).collect()
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
        let parent = tasks.create("parent", Planning::Backlog).unwrap();
        let child = subtask(&tasks, &parent, "child");
        let grandchild = subtask(&tasks, &child, "grandchild");
        subtask(&tasks, &parent, "second");
        assert_eq!(child.planning, Planning::Backlog);
        assert_eq!(grandchild.parent.as_deref(), Some(child.id.as_str()));
        assert!(titles(&tasks, Planning::Active).is_empty());
        let listed = tasks.tasks(Planning::Backlog).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(sub_titles(&listed[0]), ["child", "second"]);
        assert_eq!(sub_titles(&listed[0].subtasks[0]), ["grandchild"]);
    }

    #[test]
    fn a_subtask_needs_an_open_parent_and_a_title() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active).unwrap();
        assert!(matches!(
            tasks.create_subtask(&parent.id, " "),
            Err(Error::EmptyTitle)
        ));
        assert!(matches!(
            tasks.create_subtask("aaaaaaaaaaaa", "x"),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
        raw(&dir, "bbbbbbbbbbbb", "done", None, 1);
        assert!(tasks.create_subtask("bbbbbbbbbbbb", "x").is_err());
    }

    #[test]
    fn planning_a_task_rewrites_its_open_subtasks_and_records_each() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog).unwrap();
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
            sub_titles(&tasks.tasks(Planning::Active).unwrap()[0]),
            ["c", "cccccccccccc"]
        );
    }

    #[test]
    fn a_subtask_cannot_be_made_active_or_moved_by_itself() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Backlog).unwrap();
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
        let parent = tasks.create("p", Planning::Active).unwrap();
        let child = subtask(&tasks, &parent, "c");
        let grandchild = subtask(&tasks, &child, "g");
        let before = (file_of(&dir, &child), history(&dir));
        tasks.close(&parent.id, Closing::Done).unwrap();
        let listed = tasks.tasks(Planning::Active).unwrap();
        assert!(listed[0].done && listed[0].subtasks[0].done);
        tasks.revert();
        tasks.commit().unwrap();
        assert_eq!((file_of(&dir, &child), history(&dir)), before);

        tasks.close(&parent.id, Closing::Rejected).unwrap();
        assert!(tasks.tasks(Planning::Active).unwrap().is_empty());
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
    fn a_closed_subtask_stays_listed_under_its_open_parent_marked_closed() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active).unwrap();
        let open = subtask(&tasks, &parent, "open");
        let gone = subtask(&tasks, &parent, "gone");
        tasks.close(&gone.id, Closing::Rejected).unwrap();
        assert_eq!(
            sub_titles(&tasks.tasks(Planning::Active).unwrap()[0]),
            ["open"]
        );
        tasks.commit().unwrap();
        let listed = tasks.tasks(Planning::Active).unwrap();
        let flags: Vec<_> = listed[0].subtasks.iter().map(|t| t.closed).collect();
        assert_eq!(flags, [false, true]);
        assert_eq!(listed[0].subtasks[0].id, open.id);
        tasks.close(&parent.id, Closing::Done).unwrap();
        tasks.commit().unwrap();
        assert!(tasks.tasks(Planning::Active).unwrap().is_empty());
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
        let listed = tasks.tasks(Planning::Active).unwrap();
        let roots: Vec<_> = listed.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(roots, ["aaaaaaaaaaaa", "dddddddddddd"]);
        assert_eq!(sub_titles(&listed[0]), ["cccccccccccc"]);
        assert_eq!(sub_titles(&listed[0].subtasks[0]), ["bbbbbbbbbbbb"]);
    }

    #[test]
    fn a_subtask_is_planned_as_its_top_level_task() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw(&dir, "aaaaaaaaaaaa", "backlog", None, 1);
        raw(&dir, "bbbbbbbbbbbb", "active", Some("aaaaaaaaaaaa"), 1);
        raw(&dir, "cccccccccccc", "active", Some("bbbbbbbbbbbb"), 1);
        assert!(titles(&tasks, Planning::Active).is_empty());
        let listed = tasks.tasks(Planning::Backlog).unwrap();
        assert_eq!(listed[0].subtasks[0].planning, Planning::Backlog);
        assert_eq!(
            listed[0].subtasks[0].subtasks[0].planning,
            Planning::Backlog
        );
    }

    #[test]
    fn a_copy_of_a_task_file_under_another_name_is_ignored() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("original", Planning::Active).unwrap();
        let copy = file_of(&dir, &task).replace("original", "copy");
        for name in [format!("{} 1.md", task.id), format!("~{}.md", task.id)] {
            fs::write(dir.0.join(name), &copy).unwrap();
        }
        tasks.edit(&task.id, "edited", "", 4).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["edited"]);
    }

    #[test]
    fn committing_skips_a_subtask_closed_or_removed_meanwhile() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("p", Planning::Active).unwrap();
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
        let [a, b, c] = ["a", "b", "c"].map(|t| tasks.create(t, Planning::Active).unwrap());
        (tasks, [a, b, c])
    }

    #[test]
    fn a_task_moves_before_a_sibling_or_to_the_end() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        tasks
            .move_to(&c.id, None, Planning::Active, Some(&a.id))
            .unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["c", "a", "b"]);
        tasks
            .move_to(&b.id, None, Planning::Active, Some(&a.id))
            .unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["c", "b", "a"]);
        tasks.move_to(&c.id, None, Planning::Active, None).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["b", "a", "c"]);
        assert_eq!(titles(&dir.store(), Planning::Active), ["b", "a", "c"]);
    }

    #[test]
    fn a_move_within_a_list_rewrites_only_the_moved_task() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        let before = (file_of(&dir, &a), file_of(&dir, &b), history(&dir));
        tasks
            .move_to(&c.id, None, Planning::Active, Some(&b.id))
            .unwrap();
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
    fn a_task_moves_under_another_and_out_again() {
        let dir = Scratch::new();
        let (tasks, [a, b, c]) = move_fixture(&dir);
        let sub = subtask(&tasks, &a, "sub");
        tasks
            .move_to(&c.id, Some(&a.id), Planning::Active, Some(&sub.id))
            .unwrap();
        let listed = tasks.tasks(Planning::Active).unwrap();
        assert_eq!(
            listed.iter().map(|t| &*t.title).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(sub_titles(&listed[0]), ["c", "sub"]);
        assert_eq!(listed[0].subtasks[0].parent.as_deref(), Some(a.id.as_str()));
        tasks
            .move_to(&sub.id, None, Planning::Active, Some(&b.id))
            .unwrap();
        let listed = tasks.tasks(Planning::Active).unwrap();
        assert_eq!(
            listed.iter().map(|t| &*t.title).collect::<Vec<_>>(),
            ["a", "sub", "b"]
        );
        assert_eq!(listed[1].parent, None);
        assert!(!file_of(&dir, &sub).contains("parent"));
    }

    #[test]
    fn moving_into_its_own_subtree_is_refused_and_changes_nothing() {
        let dir = Scratch::new();
        let (tasks, [a, ..]) = move_fixture(&dir);
        let child = subtask(&tasks, &a, "child");
        let grandchild = subtask(&tasks, &child, "grandchild");
        let before = (file_of(&dir, &a), history(&dir));
        for under in [&a, &child, &grandchild] {
            assert!(matches!(
                tasks.move_to(&a.id, Some(&under.id), Planning::Active, None),
                Err(Error::MoveIntoOwnSubtree)
            ));
        }
        assert_eq!((file_of(&dir, &a), history(&dir)), before);
    }

    #[test]
    fn a_moved_task_takes_its_new_top_level_tasks_list_and_records_it() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let active = tasks.create("active", Planning::Active).unwrap();
        let backlog = tasks.create("backlog", Planning::Backlog).unwrap();
        let own = subtask(&tasks, &active, "own");
        let before = history(&dir).len();
        tasks
            .move_to(&active.id, Some(&backlog.id), Planning::Active, None)
            .unwrap();
        for task in [&active, &own] {
            assert!(file_of(&dir, task).contains("state: backlog"));
        }
        let lines = history(&dir);
        assert_eq!(lines.len(), before + 2);
        assert!(
            lines[before..]
                .iter()
                .all(|l| l.contains(" moved-to-backlog "))
        );
        let listed = tasks.tasks(Planning::Backlog).unwrap();
        assert_eq!(sub_titles(&listed[0]), ["active"]);
        assert_eq!(sub_titles(&listed[0].subtasks[0]), ["own"]);

        tasks
            .move_to(&active.id, None, Planning::Active, None)
            .unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["active"]);
        assert!(file_of(&dir, &own).contains("state: active"));
        assert_eq!(history(&dir).len(), before + 4);
    }

    #[test]
    fn moving_with_an_unknown_or_foreign_target_fails() {
        let dir = Scratch::new();
        let (tasks, [a, b, _]) = move_fixture(&dir);
        let sub = subtask(&tasks, &a, "sub");
        raw(&dir, "dddddddddddd", "done", None, 1);
        for (id, parent, before) in [
            ("aaaaaaaaaaaa", None, None),
            ("dddddddddddd", None, None),
            (&b.id[..], Some("aaaaaaaaaaaa"), None),
            (&b.id[..], Some("dddddddddddd"), None),
            (&b.id[..], None, Some(&sub.id[..])),
            (&b.id[..], None, Some(&b.id[..])),
        ] {
            assert!(
                matches!(
                    tasks.move_to(id, parent, Planning::Active, before),
                    Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
                ),
                "{id} {parent:?} {before:?}"
            );
        }
    }

    fn two_tasks(tasks: &Tasks) -> (Task, Task) {
        (
            tasks.create("a", Planning::Active).unwrap(),
            tasks.create("b", Planning::Backlog).unwrap(),
        )
    }

    fn listed(tasks: &Tasks, id: &str) -> Task {
        [Planning::Active, Planning::Backlog]
            .into_iter()
            .flat_map(|p| tasks.tasks(p).unwrap())
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
        let c = tasks.create("c", Planning::Active).unwrap();
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
        let c = tasks.create("c", Planning::Active).unwrap();
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
        let a = tasks.create("a", Planning::Active).unwrap();
        let sub = tasks.create_subtask(&a.id, "sub").unwrap();
        let b = tasks.create("b", Planning::Backlog).unwrap();
        let c = tasks.create("c", Planning::Active).unwrap();
        let d = tasks.create("d", Planning::Backlog).unwrap();
        let e = tasks.create("e", Planning::Active).unwrap();
        let gone = tasks.create("gone", Planning::Active).unwrap();
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
        tasks.edit("aaaaaaaaaaaa", "t", "", 4).unwrap();
        assert!(!file_of(&dir, &a).contains("blocked-by"));
    }

    #[test]
    fn a_blocked_by_loop_left_by_sync_is_tolerated() {
        let dir = Scratch::new();
        let tasks = dir.store();
        raw_blocked(&dir, "aaaaaaaaaaaa", "active", &["bbbbbbbbbbbb"]);
        raw_blocked(&dir, "bbbbbbbbbbbb", "active", &["aaaaaaaaaaaa"]);
        raw_blocked(&dir, "cccccccccccc", "active", &["cccccccccccc"]);
        let listed = tasks.tasks(Planning::Active).unwrap();
        assert_eq!(listed.len(), 3);
        assert!(listed.iter().all(|t| t.blocked));
        let d = tasks.create("d", Planning::Active).unwrap();
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
        let parent = tasks.create("parent", Planning::Active).unwrap();
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
        let before = tasks.tasks(Planning::Active).unwrap();
        lock_files(&dir);
        assert_eq!(tasks.tasks(Planning::Active).unwrap(), before);
    }

    #[cfg(unix)]
    #[test]
    fn closed_tasks_are_remembered_across_launches() {
        let dir = Scratch::new();
        let (tasks, gone) = cache_fixture(&dir);
        tasks.tasks(Planning::Active).unwrap();
        lock_file(&dir.0.join(format!("{}.md", gone.id)));
        let listed = dir.store().tasks(Planning::Active).unwrap();
        let closed = &listed[0].subtasks[0];
        assert_eq!(
            (closed.id.as_str(), closed.title.as_str()),
            (&*gone.id, "gone")
        );
        assert!(closed.closed);
    }

    #[test]
    fn a_file_changed_added_or_removed_outside_the_app_shows_up() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let kept = tasks.create("kept", Planning::Active).unwrap();
        let other = tasks.create("other", Planning::Active).unwrap();
        assert_eq!(titles(&tasks, Planning::Active), ["kept", "other"]);

        let path = dir.0.join(format!("{}.md", kept.id));
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("kept", "changed");
        fs::write(&path, text).unwrap();
        raw(&dir, "aaaaaaaaaaaa", "active", None, 0);
        fs::remove_file(dir.0.join(format!("{}.md", other.id))).unwrap();
        assert_eq!(
            titles(&tasks, Planning::Active),
            ["aaaaaaaaaaaa", "changed"]
        );
        assert_eq!(
            titles(&dir.store(), Planning::Active),
            ["aaaaaaaaaaaa", "changed"]
        );
    }

    #[test]
    fn a_task_closed_by_another_device_is_no_longer_open() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tasks.create("t", Planning::Active).unwrap();
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
        tasks.tasks(Planning::Active).unwrap();
        fs::write(dir.cache(), "nonsense\n\t\t\n").unwrap();
        assert_eq!(dir.store().tasks(Planning::Active).unwrap().len(), 1);
    }

    #[test]
    fn the_cache_is_kept_outside_the_synced_folder() {
        let dir = Scratch::new();
        let (tasks, _) = cache_fixture(&dir);
        tasks.tasks(Planning::Active).unwrap();
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
        let parent = tasks.tasks(Planning::Active).unwrap().remove(0);
        fs::remove_file(dir.cache()).unwrap();
        tasks.edit(&parent.id, "renamed", "", 1).unwrap();
        tasks.create("new", Planning::Active).unwrap();
        tasks.tasks(Planning::Active).unwrap();
        assert!(!dir.cache().exists());
        tasks.close(&parent.id, Closing::Rejected).unwrap();
        tasks.commit().unwrap();
        tasks.tasks(Planning::Active).unwrap();
        assert!(dir.cache().exists());
    }

    #[test]
    fn rows_list_each_task_before_its_subtasks() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let parent = tasks.create("parent", Planning::Active).unwrap();
        tasks.create_subtask(&parent.id, "child").unwrap();
        tasks.create("other", Planning::Active).unwrap();
        let rows: Vec<_> = tasks
            .rows(Planning::Active)
            .unwrap()
            .into_iter()
            .map(|r| (r.task.title, r.depth, r.has_subtasks))
            .collect();
        assert_eq!(
            rows,
            [
                ("parent".to_owned(), 0, true),
                ("child".to_owned(), 1, false),
                ("other".to_owned(), 0, false)
            ]
        );
    }

    #[test]
    fn tasks_live_on_any_files() {
        let files = Arc::new(crate::files::MemoryFiles::default());
        let device = ItemId::parse("dddddddddddd").unwrap();
        let tasks = Tasks::open_in(files.clone(), "Tasks", device, None);
        let task = tasks.create("Buy milk", Planning::Backlog).unwrap();
        tasks.add_tag(&task.id, "home").unwrap();
        tasks.plan(&task.id, Planning::Active).unwrap();
        assert_eq!(tasks.tags_with_prefix("h").unwrap(), ["home"]);
        let listed = tasks.tasks(Planning::Active).unwrap();
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
