//! The Tasks surface of the wasm exports, mirroring `crates/bindings/src/tasks.rs`.
//!
//! The browser's storage is asynchronous, so the store works on files in
//! memory: the client hands in the files it has saved when opening, and after
//! each change saves the files `take_changes` names.

use pkm_tasks::{Files, MemoryFiles, Row, Tasks};
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

mirror_enum!(
    /// Where an Open Task is planned.
    Planning in pkm_tasks::Planning { Active, Backlog }
);

mirror_enum!(
    /// How a Task is Closed.
    Closing in pkm_tasks::Closing { Done, Rejected }
);

/// A Task another Task is Blocked by; a `closed` one no longer blocks.
#[wasm_bindgen(getter_with_clone)]
pub struct Blocker {
    pub id: String,
    pub title: String,
    pub closed: bool,
}

/// A Tag that exists, with its Tag Color.
#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct TagEntry {
    pub name: String,
    /// `#rrggbb`; none is Default.
    pub color: Option<String>,
}

impl From<pkm_base::tags::Entry> for TagEntry {
    fn from(e: pkm_base::tags::Entry) -> Self {
        Self {
            name: e.name,
            color: e.color,
        }
    }
}

/// What a Tag field offers for the typed text.
#[wasm_bindgen(getter_with_clone)]
pub struct TagSuggestions {
    /// Known Tags: prefix matches, then matches anywhere, each sorted.
    pub matches: Vec<String>,
    /// Whether to offer "Create Tag" for the typed text.
    pub create: bool,
}

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
    blocked_by: Vec<pkm_tasks::Blocker>,
    tags: Vec<TagEntry>,
}

#[wasm_bindgen]
impl ListedTask {
    /// The Tags, in the order they were added, each in its Tag Color.
    pub fn tags(&self) -> Vec<TagEntry> {
        self.tags.clone()
    }

    /// The Tasks it's Blocked by, Open or Closed.
    #[wasm_bindgen(js_name = blockedBy)]
    pub fn blocked_by(&self) -> Vec<Blocker> {
        let blockers = self.blocked_by.iter();
        blockers
            .map(|b| Blocker {
                id: b.id.clone(),
                title: b.title.clone(),
                closed: b.closed,
            })
            .collect()
    }
}

impl ListedTask {
    /// `row` with each of its Tags in the Tag Color `known` gives it.
    fn new(row: Row, known: &pkm_base::tags::Tags) -> Self {
        let t = row.task;
        let tags = t.tags.into_iter().map(|name| TagEntry {
            color: known.color(&name).map(str::to_owned),
            name,
        });
        Self {
            tags: tags.collect(),
            id: t.id,
            title: t.title,
            priority: t.priority,
            planning: t.planning.into(),
            description: t.description,
            parent: t.parent,
            depth: row.depth,
            has_subtasks: row.has_subtasks,
            done: t.done,
            closed: t.closed,
            blocked: t.blocked,
            blocked_by: t.blocked_by,
        }
    }
}

/// The Tasks of this Device, see `pkm_tasks::Tasks`.
#[wasm_bindgen]
pub struct TaskStore {
    files: Arc<MemoryFiles>,
    tasks: Tasks,
}

impl TaskStore {
    /// `rows` as the lists show them.
    fn listed(&self, rows: Vec<Row>) -> Vec<ListedTask> {
        let known = self.tasks.tags();
        rows.into_iter()
            .map(|r| ListedTask::new(r, &known))
            .collect()
    }
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

    /// The paths written since the last call; `file` holds what to save.
    #[wasm_bindgen(js_name = takeChanges)]
    pub fn take_changes(&self) -> Vec<String> {
        let changes = self.files.take_changes();
        changes
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// What the file at `path` holds now; none if there is no such file.
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

    /// Creates a Task, refusing an empty Title; its Item ID.
    pub fn create(&self, title: &str, planning: Planning) -> Result<String, JsError> {
        self.tasks
            .create(title, planning.into())
            .map(|task| task.id)
            .map_err(error)
    }

    /// Creates a Subtask under a Task, refusing an empty Title; it follows its
    /// top-level Task's Active or Backlog; its Item ID.
    #[wasm_bindgen(js_name = createSubtask)]
    pub fn create_subtask(&self, parent: &str, title: &str) -> Result<String, JsError> {
        self.tasks
            .create_subtask(parent, title)
            .map(|task| task.id)
            .map_err(error)
    }

    /// Moves a Task under `parent`, or to the top level of the `planning` list,
    /// before its sibling `before` or last; see `pkm_tasks::Tasks::move_to`.
    #[wasm_bindgen(js_name = moveTo)]
    pub fn move_to(
        &self,
        id: &str,
        parent: Option<String>,
        planning: Planning,
        before: Option<String>,
    ) -> Result<(), JsError> {
        self.tasks
            .move_to(id, parent.as_deref(), planning.into(), before.as_deref())
            .map(drop)
            .map_err(error)
    }

    /// Closes a Task as a pending change, committing the previous one first.
    /// Nothing is saved until `commit`; `revert` takes it back.
    pub fn close(&self, id: &str, closing: Closing) -> Result<(), JsError> {
        self.tasks.close(id, closing.into()).map_err(error)
    }

    /// Drops the pending Close, leaving no trace.
    pub fn revert(&self) {
        self.tasks.revert();
    }

    /// Saves the pending Close, if any.
    pub fn commit(&self) -> Result<(), JsError> {
        self.tasks.commit().map_err(error)
    }

    /// Marks a Task as Blocked by another Open Task; refused for itself, its
    /// Parent, its Subtask and a loop.
    #[wasm_bindgen(js_name = addBlocker)]
    pub fn add_blocker(&self, id: &str, blocker: &str) -> Result<(), JsError> {
        self.tasks.add_blocker(id, blocker).map(drop).map_err(error)
    }

    /// The Open Tasks of both lists a Task could be Blocked by: the ones
    /// `addBlocker` accepts, minus its current Blockers.
    #[wasm_bindgen(js_name = blockerCandidates)]
    pub fn blocker_candidates(&self, id: &str) -> Result<Vec<ListedTask>, JsError> {
        Ok(self.listed(self.tasks.blocker_candidates(id).map_err(error)?))
    }

    /// Removes a Blocked by from a Task.
    #[wasm_bindgen(js_name = removeBlocker)]
    pub fn remove_blocker(&self, id: &str, blocker: &str) -> Result<(), JsError> {
        self.tasks
            .remove_blocker(id, blocker)
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

    /// What a Tag field offers for the typed `query`, leaving out the Tags the
    /// Task already has.
    #[wasm_bindgen(js_name = suggestTags)]
    pub fn suggest_tags(
        &self,
        query: &str,
        leave_out: Vec<String>,
    ) -> Result<TagSuggestions, JsError> {
        let found = self.tasks.suggest_tags(query, &leave_out).map_err(error)?;
        Ok(TagSuggestions {
            matches: found.matches,
            create: found.create,
        })
    }

    /// Every known Tag with its Tag Color, alphabetically.
    #[wasm_bindgen(js_name = allTags)]
    pub fn all_tags(&self) -> Vec<TagEntry> {
        self.tasks
            .tags()
            .list()
            .into_iter()
            .map(Into::into)
            .collect()
    }

    /// Gives a Tag its Tag Color (`#rrggbb`), or Default for none.
    #[wasm_bindgen(js_name = setTagColor)]
    pub fn set_tag_color(&self, tag: &str, color: Option<String>) -> Result<(), JsError> {
        self.tasks
            .set_tag_color(tag, color.as_deref())
            .map_err(error)
    }

    /// Deletes a Tag: Removes it from every Task, Open or Closed, then forgets
    /// it and its Tag Color. Removing first means an interrupted Delete can
    /// simply be repeated.
    #[wasm_bindgen(js_name = deleteTag)]
    pub fn delete_tag(&self, tag: &str) -> Result<(), JsError> {
        self.tasks.remove_tag_everywhere(tag).map_err(error)?;
        self.tasks.forget_tag(tag).map_err(error)
    }

    /// The Tasks planned as `planning`, in their order, each followed by its Subtasks.
    pub fn tasks(&self, planning: Planning) -> Result<Vec<ListedTask>, JsError> {
        Ok(self.listed(self.tasks.rows(planning.into()).map_err(error)?))
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
