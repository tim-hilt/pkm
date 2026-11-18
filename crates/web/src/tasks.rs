//! The Tasks surface of the wasm exports, mirroring `crates/bindings/src/tasks.rs`.
//!
//! The browser's storage is asynchronous, so the store works on files in
//! memory: the client hands in the files it has saved when opening, and after
//! each change saves the files `take_changes` names.

use pkm_tasks::{Files, FilterSort, MemoryFiles, QuickAdd, Row, ShortcutMeaning, Tasks};
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

mirror_enum!(
    /// How a Task is Closed.
    Closing in pkm_tasks::Closing { Done, Rejected }
);

mirror_enum!(
    /// What a Sort orders by.
    FilterSortKey in pkm_tasks::FilterSortKey { Manual, Priority }
);

mirror_enum!(
    /// Which end of a Sort key comes first.
    Direction in pkm_tasks::Direction { Ascending, Descending }
);

/// Every Sort key, in the order a menu lists them.
#[wasm_bindgen(js_name = sortKeys)]
pub fn sort_keys() -> Vec<FilterSortKey> {
    pkm_tasks::FilterSortKey::ALL
        .into_iter()
        .map(Into::into)
        .collect()
}

/// A Task another Task is Blocked by; a `closed` one no longer blocks.
#[wasm_bindgen(getter_with_clone)]
pub struct Blocker {
    pub id: String,
    pub title: String,
    pub closed: bool,
}

/// A Filter: a Title and a Query, see `pkm_tasks::Filter`.
#[wasm_bindgen(getter_with_clone)]
pub struct Filter {
    pub id: String,
    pub title: String,
    /// As the user typed it.
    pub query: String,
    pub favorite: bool,
    /// What the Filter is Sorted by.
    #[wasm_bindgen(js_name = sortKey)]
    pub sort_key: FilterSortKey,
    /// Which end of the key comes first; none for Manual.
    #[wasm_bindgen(js_name = sortDirection)]
    pub sort_direction: Option<Direction>,
    /// How many Open Tasks the Query matches.
    pub matches: u32,
    /// Why the stored Query can't be read; such a Filter matches nothing.
    pub error: Option<String>,
}

impl From<pkm_tasks::Filter> for Filter {
    fn from(f: pkm_tasks::Filter) -> Self {
        Self {
            id: f.id,
            title: f.title,
            query: f.query,
            favorite: f.favorite,
            sort_key: f.sort.key().into(),
            sort_direction: match f.sort {
                FilterSort::Manual => None,
                FilterSort::Priority(direction) => Some(direction.into()),
            },
            matches: f.matches,
            error: f.error,
        }
    }
}

/// What is wrong with a Query text, and where.
#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct QueryProblem {
    /// In characters from the start of the text.
    pub position: u32,
    pub message: String,
}

/// What checking a Query text found.
#[wasm_bindgen(getter_with_clone)]
pub struct QueryCheck {
    /// Why it can't be read, if it can't.
    pub problem: Option<QueryProblem>,
    /// Tags it names that don't exist: allowed, but worth marking.
    pub unknown: Vec<String>,
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

/// A Shortcut in a Quick Add text, at `start..end` in UTF-16 code units. It
/// sets a Priority or adds a Tag, never both.
#[wasm_bindgen(getter_with_clone)]
pub struct QuickShortcut {
    pub start: u32,
    pub end: u32,
    /// The Priority it sets, 1 to 4.
    pub priority: Option<u8>,
    /// The Tag it adds, without the `@`.
    pub tag: Option<String>,
}

/// What to suggest while the cursor is in an `@` word.
#[wasm_bindgen(getter_with_clone)]
pub struct TagQuery {
    /// The word after its `@`; empty for a lone `@`.
    pub query: String,
    /// Tags typed as Shortcuts in other words, which need no suggesting again.
    #[wasm_bindgen(js_name = leaveOut)]
    pub leave_out: Vec<String>,
}

/// The text of a Title field with Shortcuts in it, see `pkm_tasks::QuickAdd`.
/// One per field.
#[wasm_bindgen]
pub struct TaskQuickAdd(QuickAdd);

#[wasm_bindgen]
impl TaskQuickAdd {
    /// An empty text, for a new Task.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self(QuickAdd::new())
    }

    /// The text of a saved Title, for editing, with every Shortcut-like part
    /// Ignored.
    #[wasm_bindgen(js_name = fromTitle)]
    pub fn from_title(title: &str) -> Self {
        Self(QuickAdd::from_title(title))
    }

    /// Replaces the UTF-16 range `start..end` with `with` and moves the cursor
    /// behind it.
    pub fn replace(&mut self, start: u32, end: u32, with: &str) {
        self.0.replace(start, end, with);
    }

    /// Ignores the Shortcut at a UTF-16 position, keeping it as plain text.
    pub fn ignore(&mut self, at: u32) {
        self.0.ignore(at);
    }

    /// Ignores every Priority Shortcut, as when the Priority is picked by hand.
    #[wasm_bindgen(js_name = ignorePriority)]
    pub fn ignore_priority(&mut self) {
        self.0.ignore_priority();
    }

    /// Puts the cursor at a UTF-16 position.
    #[wasm_bindgen(js_name = moveCursor)]
    pub fn move_cursor(&mut self, at: u32) {
        self.0.move_cursor(at);
    }

    /// Where the cursor is, in UTF-16 code units.
    pub fn cursor(&self) -> u32 {
        self.0.cursor()
    }

    /// The text as typed.
    pub fn text(&self) -> String {
        self.0.text()
    }

    /// The Shortcuts to draw highlighted.
    pub fn highlights(&self) -> Vec<QuickShortcut> {
        let found = self.0.highlights().into_iter();
        found
            .map(|s| {
                let (priority, tag) = match s.meaning {
                    ShortcutMeaning::Priority(p) => (Some(p), None),
                    ShortcutMeaning::Tag(t) => (None, Some(t)),
                };
                QuickShortcut {
                    start: s.start,
                    end: s.end,
                    priority,
                    tag,
                }
            })
            .collect()
    }

    /// The Title to save; empty if only Shortcuts were typed.
    pub fn title(&self) -> String {
        self.0.title()
    }

    /// The Priority to apply, if a Shortcut sets one.
    pub fn priority(&self) -> Option<u8> {
        self.0.priority()
    }

    /// The Tags to add.
    pub fn tags(&self) -> Vec<String> {
        self.0.tags()
    }

    /// What to suggest, if the cursor is in an `@` word.
    #[wasm_bindgen(js_name = tagQuery)]
    pub fn tag_query(&self) -> Option<TagQuery> {
        self.0.tag_query().map(|q| TagQuery {
            query: q.query,
            leave_out: q.leave_out,
        })
    }

    /// Completes the `@` word under the cursor to `@tag `, cursor behind it.
    #[wasm_bindgen(js_name = pickTag)]
    pub fn pick_tag(&mut self, tag: &str) {
        self.0.pick_tag(tag);
    }
}

impl Default for TaskQuickAdd {
    fn default() -> Self {
        Self::new()
    }
}

/// The Tasks a Filter shows under one heading.
#[wasm_bindgen(getter_with_clone)]
pub struct ListedGroup {
    /// The Tag the Group is headed by; none for the Tasks of no Group.
    pub tag: Option<String>,
    tasks: Vec<ListedTask>,
}

#[wasm_bindgen]
impl ListedGroup {
    /// The Tasks shown, handed over once (a second call gets none).
    #[wasm_bindgen(js_name = takeTasks)]
    pub fn take_tasks(&mut self) -> Vec<ListedTask> {
        std::mem::take(&mut self.tasks)
    }
}

/// A Task as the lists show it, one row of a flat list.
#[wasm_bindgen(getter_with_clone)]
pub struct ListedTask {
    pub id: String,
    pub title: String,
    /// 1 (highest) to 4.
    pub priority: u8,
    /// Plain text; empty if none.
    pub description: String,
    /// The Parent's ID, if it is a Subtask.
    pub parent: Option<String>,
    /// The Parent's Title, if it is a Subtask.
    #[wasm_bindgen(js_name = parentTitle)]
    pub parent_title: Option<String>,
    /// How many of its direct Subtasks are still Open.
    #[wasm_bindgen(js_name = openSubtasks)]
    pub open_subtasks: u32,
    /// Closed as Done, the Close still pending.
    pub done: bool,
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
            description: t.description,
            parent: t.parent,
            parent_title: row.parent_title,
            open_subtasks: row.open_subtasks,
            done: t.done,
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

    /// The folder of the Filters, beside the Tasks folder.
    #[wasm_bindgen(js_name = filtersFolder)]
    pub fn filters_folder() -> String {
        Tasks::FILTERS_FOLDER.to_owned()
    }

    /// Opens the Tasks from the saved files: `paths[i]` held `texts[i]`, in
    /// storage paths (`Tasks/<id>.md`, `Filters/<id>.md`).
    #[wasm_bindgen(constructor)]
    pub fn new(paths: Vec<String>, texts: Vec<String>) -> TaskStore {
        let files = Arc::new(MemoryFiles::default());
        for (path, text) in paths.into_iter().zip(texts) {
            files.load(Path::new(&path), text);
        }
        let tasks = Tasks::open_in(files.clone(), Tasks::FOLDER);
        Self { files, tasks }
    }

    /// The paths written or removed since the last call; `file` holds what to
    /// save, none for a removed one.
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

    /// Creates a Task with a Priority and Tags, refusing an empty Title or an
    /// invalid Tag; its Item ID.
    pub fn create(&self, title: &str, priority: u8, tags: Vec<String>) -> Result<String, JsError> {
        self.tasks
            .create(title, priority, &tags)
            .map(|task| task.id)
            .map_err(error)
    }

    /// The Tags a Task needs to stay in the Filter `id`, which a new Task
    /// starts with; none if there is no such Filter.
    #[wasm_bindgen(js_name = requiredTags)]
    pub fn required_tags(&self, id: &str) -> Result<Vec<String>, JsError> {
        self.tasks.required_tags(id).map_err(error)
    }

    /// `create` for a Subtask under a Task; it starts with its Parent's Tags.
    #[wasm_bindgen(js_name = createSubtask)]
    pub fn create_subtask(
        &self,
        parent: &str,
        title: &str,
        priority: u8,
        tags: Vec<String>,
    ) -> Result<String, JsError> {
        self.tasks
            .create_subtask(parent, title, priority, &tags)
            .map(|task| task.id)
            .map_err(error)
    }

    /// Moves a Subtask before its sibling `before`, or after its last sibling;
    /// its Parent stays. See `pkm_tasks::Tasks::move_subtask`.
    #[wasm_bindgen(js_name = moveSubtask)]
    pub fn move_subtask(&self, id: &str, before: Option<String>) -> Result<(), JsError> {
        self.tasks
            .move_subtask(id, before.as_deref())
            .map(drop)
            .map_err(error)
    }

    /// Closes a Task as a pending change, committing the previous one first.
    /// Nothing is Deleted until `commit`; `revert` takes it back.
    pub fn close(&self, id: &str, closing: Closing) -> Result<(), JsError> {
        self.tasks.close(id, closing.into()).map_err(error)
    }

    /// Drops the pending Close, leaving no trace.
    pub fn revert(&self) {
        self.tasks.revert();
    }

    /// Deletes the files of the pending Close, if any.
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

    /// Sets a Task's Title, Description and Priority and adds Tags in one
    /// write, refusing an empty Title or an invalid Tag.
    pub fn edit(
        &self,
        id: &str,
        title: &str,
        description: &str,
        priority: u8,
        add_tags: Vec<String>,
    ) -> Result<(), JsError> {
        self.tasks
            .edit(id, title, description, priority, &add_tags)
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

    /// Every Open Task, Subtasks included, in the order of All Tasks.
    pub fn tasks(&self) -> Result<Vec<ListedTask>, JsError> {
        Ok(self.listed(self.tasks.rows().map_err(error)?))
    }

    /// Creates a Filter at the end of the Filters list, refusing an empty
    /// Title or a Query that can't be read.
    #[wasm_bindgen(js_name = createFilter)]
    pub fn create_filter(&self, title: &str, query: &str) -> Result<Filter, JsError> {
        self.tasks
            .create_filter(title, query)
            .map(Into::into)
            .map_err(error)
    }

    /// Changes a Filter's Title and Query, refusing what `createFilter` does.
    #[wasm_bindgen(js_name = editFilter)]
    pub fn edit_filter(&self, id: &str, title: &str, query: &str) -> Result<Filter, JsError> {
        self.tasks
            .edit_filter(id, title, query)
            .map(Into::into)
            .map_err(error)
    }

    /// Marks a Filter as a Favorite, or unmarks it.
    #[wasm_bindgen(js_name = setFavorite)]
    pub fn set_favorite(&self, id: &str, favorite: bool) -> Result<(), JsError> {
        self.tasks.set_favorite(id, favorite).map_err(error)
    }

    /// Moves a Filter before another, or to the end of the Filters list.
    #[wasm_bindgen(js_name = moveFilter)]
    pub fn move_filter(&self, id: &str, before: Option<String>) -> Result<(), JsError> {
        self.tasks.move_filter(id, before.as_deref()).map_err(error)
    }

    /// Deletes a Filter; no Task changes.
    #[wasm_bindgen(js_name = deleteFilter)]
    pub fn delete_filter(&self, id: &str) -> Result<(), JsError> {
        self.tasks.delete_filter(id).map_err(error)
    }

    /// Every Filter with its match count, in the Filters list's order.
    pub fn filters(&self) -> Result<Vec<Filter>, JsError> {
        let found = self.tasks.filters().map_err(error)?;
        Ok(found.into_iter().map(Into::into).collect())
    }

    /// A Filter's Groups in display order, each with the Open Tasks it shows.
    #[wasm_bindgen(js_name = filterGroups)]
    pub fn filter_groups(&self, id: &str) -> Result<Vec<ListedGroup>, JsError> {
        let groups = self.tasks.filter_groups(id).map_err(error)?;
        Ok(groups
            .into_iter()
            .map(|g| ListedGroup {
                tag: g.tag,
                tasks: self.listed(g.rows),
            })
            .collect())
    }

    /// Moves a Task before another in a Filter's own order, or to its end.
    #[wasm_bindgen(js_name = moveInFilter)]
    pub fn move_in_filter(
        &self,
        id: &str,
        task: &str,
        before: Option<String>,
    ) -> Result<(), JsError> {
        self.tasks
            .move_in_filter(id, task, before.as_deref())
            .map_err(error)
    }

    /// Sorts a Filter by `key`; Manual keeps the order shown, Priority from
    /// Manual discards it. A key with a Direction takes `direction`, or its
    /// default (P1 first) if none. No Task changes.
    #[wasm_bindgen(js_name = setSort)]
    pub fn set_sort(
        &self,
        id: &str,
        key: FilterSortKey,
        direction: Option<Direction>,
    ) -> Result<(), JsError> {
        let sort = match (key, direction) {
            (FilterSortKey::Priority, Some(direction)) => FilterSort::Priority(direction.into()),
            (key, _) => FilterSort::Manual.with_key(key.into()),
        };
        self.tasks.set_sort(id, sort).map_err(error)
    }

    /// Checks a Query text without saving: what is wrong with it, or which of
    /// its Tags don't exist.
    #[wasm_bindgen(js_name = checkQuery)]
    pub fn check_query(&self, text: &str) -> QueryCheck {
        match self.tasks.check_query(text) {
            Ok(unknown) => QueryCheck {
                problem: None,
                unknown,
            },
            Err(e) => QueryCheck {
                problem: Some(QueryProblem {
                    position: u32::try_from(e.position).unwrap_or(u32::MAX),
                    message: e.message,
                }),
                unknown: Vec::new(),
            },
        }
    }

    /// The direct Subtasks of an Open Task, in either list: Open or pending
    /// Done, in their manual order.
    pub fn subtasks(&self, parent: &str) -> Result<Vec<ListedTask>, JsError> {
        Ok(self.listed(self.tasks.subtasks(parent).map_err(error)?))
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

/// `text` as a Query may be typed: lowercase, without what neither a Tag nor
/// the Query syntax can hold.
#[wasm_bindgen(js_name = typedQuery)]
pub fn typed_query(text: &str) -> String {
    pkm_tasks::typed_query(text)
}

/// The Tag being typed at the end of a Query `text`, to suggest Tags for; none
/// if it doesn't end in one.
#[wasm_bindgen(js_name = unfinishedQueryTag)]
pub fn unfinished_query_tag(text: &str) -> Option<String> {
    pkm_tasks::unfinished_tag(text).map(str::to_owned)
}
