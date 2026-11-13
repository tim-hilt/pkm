//! The Tasks surface of the UniFFI library (ADR-0007): the Tasks store. The
//! logic lives in `pkm-tasks`.

use pkm_tasks::{Error, Planning, Task, Tasks};

/// Why a Task wasn't saved or the Tasks couldn't be read.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TasksError {
    EmptyTitle,
    InvalidTag,
    Failed(String),
}

impl std::fmt::Display for TasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => Error::EmptyTitle.fmt(f),
            Self::InvalidTag => Error::InvalidTag.fmt(f),
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

/// An Open Task as the lists show it, see `pkm_tasks::Task` (named apart from
/// Swift's `Task`).
#[derive(uniffi::Record)]
pub struct OpenTask {
    pub id: String,
    pub title: String,
    /// 1 (highest) to 4.
    pub priority: u8,
    pub planning: Planning,
    /// Plain text; empty if none.
    pub description: String,
    /// In the order they were added.
    pub tags: Vec<String>,
}

impl From<Task> for OpenTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning,
            description: t.description,
            tags: t.tags,
        }
    }
}

/// The Tasks in a folder, see `pkm_tasks::Tasks`.
#[derive(uniffi::Object)]
pub struct TaskStore(Tasks);

#[uniffi::export]
impl TaskStore {
    /// Opens the Tasks in the folder `root`, creating it if needed.
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<Self, TasksError> {
        Ok(Self(Tasks::open(root)?))
    }

    /// Creates a Task, refusing an empty Title.
    pub fn create(&self, title: String, planning: Planning) -> Result<OpenTask, TasksError> {
        Ok(self.0.create(&title, planning)?.into())
    }

    /// Sets a Task's Title, Description and Priority, refusing an empty Title.
    pub fn edit(
        &self,
        id: String,
        title: String,
        description: String,
        priority: u8,
    ) -> Result<OpenTask, TasksError> {
        Ok(self.0.edit(&id, &title, &description, priority)?.into())
    }

    /// Adds a Tag to a Task, refusing an invalid one.
    pub fn add_tag(&self, id: String, tag: String) -> Result<OpenTask, TasksError> {
        Ok(self.0.add_tag(&id, &tag)?.into())
    }

    /// Removes a Tag from a Task.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<OpenTask, TasksError> {
        Ok(self.0.remove_tag(&id, &tag)?.into())
    }

    /// The known Tags starting with `prefix`, sorted.
    pub fn tags_with_prefix(&self, prefix: String) -> Result<Vec<String>, TasksError> {
        Ok(self.0.tags_with_prefix(&prefix)?)
    }

    /// The Tasks planned as `planning`, in their order.
    pub fn tasks(&self, planning: Planning) -> Result<Vec<OpenTask>, TasksError> {
        Ok(self
            .0
            .tasks(planning)?
            .into_iter()
            .map(Into::into)
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
