//! The Tasks surface of the UniFFI library (ADR-0007): the Tasks store. The
//! logic lives in `pkm-tasks`.

use pkm_tasks::{Error, Planning, Task, Tasks};

/// Why a Task wasn't saved or the Tasks couldn't be read.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TasksError {
    EmptyTitle,
    Failed(String),
}

impl std::fmt::Display for TasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => Error::EmptyTitle.fmt(f),
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
}

impl From<Task> for OpenTask {
    fn from(t: Task) -> Self {
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning,
            description: t.description,
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
