//! The Tasks surface of the UniFFI library (ADR-0007): the Tasks store. The
//! logic lives in `pkm-tasks`.

use pkm_tasks::{Closing, Error, Planning, Task, Tasks};

/// Why a Task wasn't saved or the Tasks couldn't be read.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TasksError {
    EmptyTitle,
    InvalidTag,
    SubtaskNotPlannable,
    Failed(String),
}

impl std::fmt::Display for TasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => Error::EmptyTitle.fmt(f),
            Self::InvalidTag => Error::InvalidTag.fmt(f),
            Self::SubtaskNotPlannable => Error::SubtaskNotPlannable.fmt(f),
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
    /// In the order they were added.
    pub tags: Vec<String>,
    /// Closed as Done, the Close still pending: shown in place, marked.
    pub done: bool,
    /// A Closed Subtask of an Open Task: shown struck through.
    pub closed: bool,
    /// How deep it sits in the tree, 0 for a top-level Task.
    pub depth: u32,
    /// Whether it has Subtasks, which follow it in the list.
    pub has_subtasks: bool,
}

impl ListedTask {
    /// `task` and its Subtasks, each directly after its Parent.
    fn flatten(mut task: Task, depth: u32, rows: &mut Vec<Self>) {
        let subtasks = std::mem::take(&mut task.subtasks);
        let mut row = Self::from(task);
        row.depth = depth;
        row.has_subtasks = !subtasks.is_empty();
        rows.push(row);
        for subtask in subtasks {
            Self::flatten(subtask, depth + 1, rows);
        }
    }
}

impl From<Task> for ListedTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning,
            description: t.description,
            tags: t.tags,
            done: t.done,
            closed: t.closed,
            depth: 0,
            has_subtasks: false,
        }
    }
}

/// The Tasks in a folder, see `pkm_tasks::Tasks`.
#[derive(uniffi::Object)]
pub struct TaskStore(Tasks);

#[uniffi::export]
impl TaskStore {
    /// Opens the Tasks in the folder `root`, creating it if needed.
    /// `device_folder` is local to the Device and never Synced: it keeps the
    /// Device ID, which names this Device's History files.
    #[uniffi::constructor]
    pub fn open(root: String, device_folder: String) -> Result<Self, TasksError> {
        let device = pkm_base::device::id(device_folder.as_ref())?;
        Ok(Self(Tasks::open(root, device)?))
    }

    /// Makes a Task Active or moves it to the Backlog, at the end of that list.
    pub fn plan(&self, id: String, planning: Planning) -> Result<ListedTask, TasksError> {
        Ok(self.0.plan(&id, planning)?.into())
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
        Ok(self.0.create(&title, planning)?.into())
    }

    /// Creates a Subtask under a Task, refusing an empty Title; it follows its
    /// top-level Task's Active or Backlog.
    pub fn create_subtask(&self, parent: String, title: String) -> Result<ListedTask, TasksError> {
        Ok(self.0.create_subtask(&parent, &title)?.into())
    }

    /// Sets a Task's Title, Description and Priority, refusing an empty Title.
    pub fn edit(
        &self,
        id: String,
        title: String,
        description: String,
        priority: u8,
    ) -> Result<ListedTask, TasksError> {
        Ok(self.0.edit(&id, &title, &description, priority)?.into())
    }

    /// Adds a Tag to a Task, refusing an invalid one.
    pub fn add_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.0.add_tag(&id, &tag)?.into())
    }

    /// Removes a Tag from a Task.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.0.remove_tag(&id, &tag)?.into())
    }

    /// The known Tags starting with `prefix`, sorted.
    pub fn tags_with_prefix(&self, prefix: String) -> Result<Vec<String>, TasksError> {
        Ok(self.0.tags_with_prefix(&prefix)?)
    }

    /// The Tasks planned as `planning`, in their order, as a pre-order walk of
    /// the tree: each Task is followed by its Subtasks, marked by `depth`.
    pub fn tasks(&self, planning: Planning) -> Result<Vec<ListedTask>, TasksError> {
        let mut rows = Vec::new();
        for task in self.0.tasks(planning)? {
            ListedTask::flatten(task, 0, &mut rows);
        }
        Ok(rows)
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
