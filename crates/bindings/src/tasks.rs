//! The Tasks surface of the UniFFI library (ADR-0007): the Tasks store. The
//! logic lives in `pkm-tasks`.

use std::path::Path;

use pkm_base::tags::Entry as TagEntry;
use pkm_tasks::{Blocker, Closing, Error, Planning, Row, Task, Tasks};

/// Why a Task wasn't saved or the Tasks couldn't be read.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TasksError {
    EmptyTitle,
    InvalidTag,
    SubtaskNotPlannable,
    MoveIntoOwnSubtree,
    BlockedByItself,
    BlockedByParent,
    BlockedBySubtask,
    BlockedByLoop,
    Failed(String),
}

impl std::fmt::Display for TasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => Error::EmptyTitle.fmt(f),
            Self::InvalidTag => Error::InvalidTag.fmt(f),
            Self::SubtaskNotPlannable => Error::SubtaskNotPlannable.fmt(f),
            Self::MoveIntoOwnSubtree => Error::MoveIntoOwnSubtree.fmt(f),
            Self::BlockedByItself => Error::BlockedByItself.fmt(f),
            Self::BlockedByParent => Error::BlockedByParent.fmt(f),
            Self::BlockedBySubtask => Error::BlockedBySubtask.fmt(f),
            Self::BlockedByLoop => Error::BlockedByLoop.fmt(f),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl From<std::io::Error> for TasksError {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

impl From<Error> for TasksError {
    fn from(e: Error) -> Self {
        match e {
            Error::EmptyTitle => Self::EmptyTitle,
            Error::InvalidTag => Self::InvalidTag,
            Error::SubtaskNotPlannable => Self::SubtaskNotPlannable,
            Error::MoveIntoOwnSubtree => Self::MoveIntoOwnSubtree,
            Error::BlockedByItself => Self::BlockedByItself,
            Error::BlockedByParent => Self::BlockedByParent,
            Error::BlockedBySubtask => Self::BlockedBySubtask,
            Error::BlockedByLoop => Self::BlockedByLoop,
            Error::Io(e) => e.into(),
        }
    }
}

/// Where an Open Task is planned.
#[uniffi::remote(Enum)]
pub enum Planning {
    Active,
    Backlog,
}

/// How a Task is Closed.
#[uniffi::remote(Enum)]
pub enum Closing {
    Done,
    Rejected,
}

/// A Task another Task is Blocked by; `closed` ones no longer block.
#[uniffi::remote(Record)]
pub struct Blocker {
    pub id: String,
    pub title: String,
    pub closed: bool,
}

/// A Tag that exists, with its Tag Color.
#[uniffi::remote(Record)]
pub struct TagEntry {
    pub name: String,
    /// `#rrggbb`; none is Default.
    pub color: Option<String>,
}

/// A Task as the lists show it, one row of the flattened tree, see
/// `pkm_tasks::Task` (named apart from Swift's `Task`).
#[derive(uniffi::Record)]
pub struct ListedTask {
    pub id: String,
    pub title: String,
    /// 1 (highest) to 4.
    pub priority: u8,
    pub planning: Planning,
    /// Plain text; empty if none.
    pub description: String,
    /// In the order they were added, each with its Tag Color.
    pub tags: Vec<TagEntry>,
    /// Closed as Done, the Close still pending: shown in place, marked.
    pub done: bool,
    /// A Closed Subtask of an Open Task: shown struck through.
    pub closed: bool,
    /// The Parent's ID, if it is a Subtask.
    pub parent: Option<String>,
    /// How deep it sits in the tree, 0 for a top-level Task.
    pub depth: u32,
    /// Whether it has Subtasks, which follow it in the list.
    pub has_subtasks: bool,
    /// Whether any Task it's Blocked by is Open.
    pub blocked: bool,
    /// The Tasks it's Blocked by, Open or Closed.
    pub blocked_by: Vec<Blocker>,
}

impl From<Task> for ListedTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning,
            description: t.description,
            tags: t
                .tags
                .into_iter()
                .map(|name| TagEntry { name, color: None })
                .collect(),
            done: t.done,
            closed: t.closed,
            parent: t.parent,
            depth: 0,
            has_subtasks: false,
            blocked: t.blocked,
            blocked_by: t.blocked_by,
        }
    }
}

impl From<Row> for ListedTask {
    fn from(row: Row) -> Self {
        Self {
            depth: row.depth,
            has_subtasks: row.has_subtasks,
            ..row.task.into()
        }
    }
}

/// The Tasks in a folder, see `pkm_tasks::Tasks`.
#[derive(uniffi::Object)]
pub struct TaskStore(Tasks);

impl TaskStore {
    /// `task` with its Tags in their Tag Colors.
    fn painted(&self, task: ListedTask) -> ListedTask {
        paint(task, &self.0.tags())
    }
}

/// `task` with each Tag's Tag Color taken from `known`.
fn paint(mut task: ListedTask, known: &[TagEntry]) -> ListedTask {
    for tag in &mut task.tags {
        tag.color = known
            .iter()
            .find(|k| k.name == tag.name)
            .and_then(|k| k.color.clone());
    }
    task
}

#[uniffi::export]
impl TaskStore {
    /// Opens the Tasks in the folder `root`, creating it if needed.
    /// `device_folder` is local to the Device and never Synced: it keeps the
    /// Device ID, which names this Device's History files, and the cache of
    /// Closed Tasks.
    #[uniffi::constructor]
    pub fn open(root: String, device_folder: String) -> Result<Self, TasksError> {
        let device = pkm_base::device::id(device_folder.as_ref())?;
        let cache = Path::new(&device_folder).join("tasks-cache");
        Ok(Self(Tasks::open(root, device, cache)?))
    }

    /// Makes a Task Active or moves it to the Backlog, at the end of that list.
    pub fn plan(&self, id: String, planning: Planning) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.plan(&id, planning)?.into()))
    }

    /// Moves a Task under `parent`, or to the top level of the `planning` list,
    /// before its sibling `before` or last; see `pkm_tasks::Tasks::move_to`.
    pub fn move_to(
        &self,
        id: String,
        parent: Option<String>,
        planning: Planning,
        before: Option<String>,
    ) -> Result<ListedTask, TasksError> {
        let moved = self
            .0
            .move_to(&id, parent.as_deref(), planning, before.as_deref())?;
        Ok(self.painted(moved.into()))
    }

    /// Closes a Task as a pending change, committing the previous one first.
    /// Nothing is saved until `commit`; `revert` takes it back.
    pub fn close(&self, id: String, closing: Closing) -> Result<(), TasksError> {
        Ok(self.0.close(&id, closing)?)
    }

    /// Drops the pending Close, leaving no trace.
    pub fn revert(&self) {
        self.0.revert();
    }

    /// Saves the pending Close, if any.
    pub fn commit(&self) -> Result<(), TasksError> {
        Ok(self.0.commit()?)
    }

    /// Creates a Task, refusing an empty Title.
    pub fn create(&self, title: String, planning: Planning) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.create(&title, planning)?.into()))
    }

    /// Creates a Subtask under a Task, refusing an empty Title; it follows its
    /// top-level Task's Active or Backlog.
    pub fn create_subtask(&self, parent: String, title: String) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.create_subtask(&parent, &title)?.into()))
    }

    /// Sets a Task's Title, Description and Priority, refusing an empty Title.
    pub fn edit(
        &self,
        id: String,
        title: String,
        description: String,
        priority: u8,
    ) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.edit(&id, &title, &description, priority)?.into()))
    }

    /// Adds a Tag to a Task, refusing an invalid one.
    pub fn add_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.add_tag(&id, &tag)?.into()))
    }

    /// Removes a Tag from a Task.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.remove_tag(&id, &tag)?.into()))
    }

    /// Marks a Task as Blocked by another Open Task; refused for itself, its
    /// Parent, its Subtask and a loop.
    pub fn add_blocker(&self, id: String, blocker: String) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.add_blocker(&id, &blocker)?.into()))
    }

    /// The Open Tasks of both lists a Task could be Blocked by: the ones
    /// `add_blocker` accepts, minus its current Blockers.
    pub fn blocker_candidates(&self, id: String) -> Result<Vec<ListedTask>, TasksError> {
        let known = self.0.tags();
        let candidates = self.0.blocker_candidates(&id)?;
        Ok(candidates
            .into_iter()
            .map(|task| paint(task.into(), &known))
            .collect())
    }

    /// Removes a Blocked by from a Task.
    pub fn remove_blocker(&self, id: String, blocker: String) -> Result<ListedTask, TasksError> {
        Ok(self.painted(self.0.remove_blocker(&id, &blocker)?.into()))
    }

    /// The known Tags starting with `prefix`, sorted.
    pub fn tags_with_prefix(&self, prefix: String) -> Result<Vec<String>, TasksError> {
        Ok(self.0.tags_with_prefix(&prefix)?)
    }

    /// Gives a Tag its Tag Color (`#rrggbb`), or Default for none.
    pub fn set_tag_color(&self, tag: String, color: Option<String>) -> Result<(), TasksError> {
        Ok(self.0.set_tag_color(&tag, color.as_deref())?)
    }

    /// Every known Tag with its Tag Color, alphabetically.
    pub fn tags(&self) -> Vec<TagEntry> {
        self.0.tags()
    }

    /// The Tasks planned as `planning`, in their order, as a pre-order walk of
    /// the tree: each Task is followed by its Subtasks, marked by `depth`.
    pub fn tasks(&self, planning: Planning) -> Result<Vec<ListedTask>, TasksError> {
        let known = self.0.tags();
        let rows = self.0.rows(planning)?;
        Ok(rows
            .into_iter()
            .map(|row| paint(row.into(), &known))
            .collect())
    }
}

/// `text` without the characters a Tag can never contain, to refuse them as
/// they are typed.
#[uniffi::export]
pub fn strip_forbidden_tag_characters(text: String) -> String {
    pkm_base::tag::strip_forbidden(&text)
}

/// Whether `tag` is a complete, valid Tag.
#[uniffi::export]
pub fn is_valid_tag(tag: String) -> bool {
    pkm_base::tag::is_valid(&tag)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors(task: &ListedTask) -> Vec<(&str, Option<&str>)> {
        let tags = task.tags.iter();
        tags.map(|t| (t.name.as_str(), t.color.as_deref()))
            .collect()
    }

    #[test]
    fn listed_tasks_carry_their_tags_in_their_tag_colors() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-{}", std::process::id()));
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let store = TaskStore::open(path("Tasks"), path("Device")).unwrap();
        let task = store.create("t".into(), Planning::Active).unwrap();
        store.add_tag(task.id.clone(), "home".into()).unwrap();
        let task = store.add_tag(task.id, "work".into()).unwrap();
        assert_eq!(colors(&task), [("home", None), ("work", None)]);

        store
            .set_tag_color("work".into(), Some("#d0021b".into()))
            .unwrap();
        let listed = store.tasks(Planning::Active).unwrap();
        assert_eq!(
            colors(&listed[0]),
            [("home", None), ("work", Some("#d0021b"))]
        );

        store.set_tag_color("work".into(), None).unwrap();
        assert_eq!(
            colors(&store.tasks(Planning::Active).unwrap()[0])[1],
            ("work", None)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
