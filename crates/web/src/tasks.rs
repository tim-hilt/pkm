//! The Tasks surface of the wasm exports, mirroring `crates/bindings/src/tasks.rs`.
//!
//! The browser's storage is asynchronous, so the store works on files in
//! memory: the client hands in the files it has saved when opening, and after
//! each change saves what `take_changes` names, as `file` tells it.

use pkm_tasks::{Files, MemoryFiles, Row, Tasks};
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

mirror_enum!(
    /// Where an Open Task is planned.
    Planning in pkm_tasks::Planning { Active, Backlog }
);

/// A Task as the lists show it, one row of the flattened tree.
#[wasm_bindgen(getter_with_clone)]
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
    /// The Parent's ID, if it is a Subtask.
    pub parent: Option<String>,
    /// How deep it sits in the tree, 0 for a top-level Task.
    pub depth: u32,
    #[wasm_bindgen(js_name = hasSubtasks)]
    pub has_subtasks: bool,
    /// Closed as Done, the Close still pending.
    pub done: bool,
    /// A Closed Subtask of an Open Task.
    pub closed: bool,
    /// Whether any Task it's Blocked by is Open.
    pub blocked: bool,
}

impl From<Row> for ListedTask {
    fn from(row: Row) -> Self {
        let t = row.task;
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning.into(),
            description: t.description,
            tags: t.tags,
            parent: t.parent,
            depth: row.depth,
            has_subtasks: row.has_subtasks,
            done: t.done,
            closed: t.closed,
            blocked: t.blocked,
        }
    }
}

/// The Tasks of this Device, see `pkm_tasks::Tasks`.
#[wasm_bindgen]
pub struct TaskStore {
    files: Arc<MemoryFiles>,
    tasks: Tasks,
}

fn error(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl TaskStore {
    /// The folder `Tasks` is kept in, in the app's storage.
    pub fn folder() -> String {
        Tasks::FOLDER.to_owned()
    }

    /// Opens the Tasks from the saved files: `paths[i]` held `texts[i]`, in
    /// storage paths (`Tasks/<id>.md`). `device` is this Device's ID.
    #[wasm_bindgen(constructor)]
    pub fn new(device: &str, paths: Vec<String>, texts: Vec<String>) -> Result<TaskStore, JsError> {
        let device =
            pkm_base::item_id::ItemId::parse(device).ok_or_else(|| error("bad Device ID"))?;
        let files = Arc::new(MemoryFiles::default());
        for (path, text) in paths.into_iter().zip(texts) {
            files.load(Path::new(&path), text);
        }
        let tasks = Tasks::open_in(files.clone(), Tasks::FOLDER, device, None);
        Ok(Self { files, tasks })
    }

    /// The paths written or removed since the last call; `file` says which.
    #[wasm_bindgen(js_name = takeChanges)]
    pub fn take_changes(&self) -> Vec<String> {
        let changes = self.files.take_changes();
        changes
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// What the file at `path` holds now; none if it was removed.
    pub fn file(&self, path: &str) -> Option<String> {
        self.files.read(Path::new(path)).ok()
    }

    /// Makes a Task Active or moves it to the Backlog, at the end of that list.
    pub fn plan(&self, id: &str, planning: Planning) -> Result<(), JsError> {
        self.tasks
            .plan(id, planning.into())
            .map(drop)
            .map_err(error)
    }

    /// Creates a Task, refusing an empty Title.
    pub fn create(&self, title: &str, planning: Planning) -> Result<(), JsError> {
        self.tasks
            .create(title, planning.into())
            .map(drop)
            .map_err(error)
    }

    /// Sets a Task's Title, Description and Priority, refusing an empty Title.
    pub fn edit(
        &self,
        id: &str,
        title: &str,
        description: &str,
        priority: u8,
    ) -> Result<(), JsError> {
        self.tasks
            .edit(id, title, description, priority)
            .map(drop)
            .map_err(error)
    }

    /// Adds a Tag to a Task, refusing an invalid one.
    #[wasm_bindgen(js_name = addTag)]
    pub fn add_tag(&self, id: &str, tag: &str) -> Result<(), JsError> {
        self.tasks.add_tag(id, tag).map(drop).map_err(error)
    }

    /// Removes a Tag from a Task.
    #[wasm_bindgen(js_name = removeTag)]
    pub fn remove_tag(&self, id: &str, tag: &str) -> Result<(), JsError> {
        self.tasks.remove_tag(id, tag).map(drop).map_err(error)
    }

    /// The known Tags starting with `prefix`, sorted.
    #[wasm_bindgen(js_name = tagsWithPrefix)]
    pub fn tags_with_prefix(&self, prefix: &str) -> Result<Vec<String>, JsError> {
        self.tasks.tags_with_prefix(prefix).map_err(error)
    }

    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    pub fn tasks(&self, planning: Planning) -> Result<Vec<ListedTask>, JsError> {
        let rows = self.tasks.rows(planning.into()).map_err(error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}

/// `text` without the characters a Tag can never contain.
#[wasm_bindgen(js_name = stripForbiddenTagCharacters)]
pub fn strip_forbidden_tag_characters(text: &str) -> String {
    pkm_base::tag::strip_forbidden(text)
}

/// Whether `tag` is a complete, valid Tag.
#[wasm_bindgen(js_name = isValidTag)]
pub fn is_valid_tag(tag: &str) -> bool {
    pkm_base::tag::is_valid(tag)
}
