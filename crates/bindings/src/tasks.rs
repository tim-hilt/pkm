//! The Tasks surface of the UniFFI library (ADR-0007): the Tasks store. The
//! logic lives in `pkm-tasks`.

use std::sync::Mutex;

use pkm_base::tags::{Entry as TagEntry, Suggestions as TagSuggestions, Tags};
use pkm_tasks::{
    Blocker, Closing, Direction, Error, Filter, FilterSort, FilterSortKey, QuickAdd, QuickShortcut,
    Row, ShortcutMeaning, TagQuery, Task, Tasks,
};

/// Why a Task wasn't saved or the Tasks couldn't be read.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum TasksError {
    EmptyTitle,
    InvalidTag,
    BlockedByItself,
    BlockedByParent,
    BlockedBySubtask,
    BlockedByLoop,
    NotASibling,
    EmptyFilterTitle,
    InvalidQuery(String),
    Failed(String),
}

impl std::fmt::Display for TasksError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyTitle => Error::EmptyTitle.fmt(f),
            Self::InvalidTag => Error::InvalidTag.fmt(f),
            Self::BlockedByItself => Error::BlockedByItself.fmt(f),
            Self::BlockedByParent => Error::BlockedByParent.fmt(f),
            Self::BlockedBySubtask => Error::BlockedBySubtask.fmt(f),
            Self::BlockedByLoop => Error::BlockedByLoop.fmt(f),
            Self::NotASibling => Error::NotASibling.fmt(f),
            Self::EmptyFilterTitle => Error::EmptyFilterTitle.fmt(f),
            Self::InvalidQuery(message) => f.write_str(message),
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
            Error::BlockedByItself => Self::BlockedByItself,
            Error::BlockedByParent => Self::BlockedByParent,
            Error::BlockedBySubtask => Self::BlockedBySubtask,
            Error::BlockedByLoop => Self::BlockedByLoop,
            Error::NotASibling => Self::NotASibling,
            Error::EmptyFilterTitle => Self::EmptyFilterTitle,
            Error::InvalidQuery(e) => Self::InvalidQuery(e.to_string()),
            Error::Io(e) => e.into(),
        }
    }
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

/// Which end of a Sort key comes first.
#[uniffi::remote(Enum)]
pub enum Direction {
    Ascending,
    Descending,
}

/// What a Sort orders by.
#[uniffi::remote(Enum)]
pub enum FilterSortKey {
    Manual,
    Priority,
}

/// How a Filter orders its Tasks, see `pkm_tasks::FilterSort`.
#[uniffi::remote(Enum)]
pub enum FilterSort {
    Manual,
    Priority(Direction),
}

/// Every Sort key, in the order a menu lists them.
#[uniffi::export]
pub fn sort_keys() -> Vec<FilterSortKey> {
    FilterSortKey::ALL.to_vec()
}

/// The key of a Sort.
#[uniffi::export]
pub fn sort_key(sort: FilterSort) -> FilterSortKey {
    sort.key()
}

/// `sort` with another key: the same Direction if it stays Priority, else the
/// key's default.
#[uniffi::export]
pub fn sort_with_key(sort: FilterSort, key: FilterSortKey) -> FilterSort {
    sort.with_key(key)
}

/// A Filter: a Title and a Query, see `pkm_tasks::Filter`.
#[uniffi::remote(Record)]
pub struct Filter {
    pub id: String,
    pub title: String,
    /// As the user typed it.
    pub query: String,
    pub favorite: bool,
    pub sort: FilterSort,
    /// How many Open Tasks the Query matches.
    pub matches: u32,
    /// Why the stored Query can't be read; such a Filter matches nothing.
    pub error: Option<String>,
}

/// What is wrong with a Query text, and where.
#[derive(uniffi::Record)]
pub struct QueryProblem {
    /// In characters from the start of the text.
    pub position: u32,
    pub message: String,
}

/// What checking a Query text found.
#[derive(uniffi::Record)]
pub struct QueryCheck {
    /// Why it can't be read, if it can't.
    pub problem: Option<QueryProblem>,
    /// Tags it names that don't exist: allowed, but worth marking.
    pub unknown: Vec<String>,
}

/// A Tag that exists, with its Tag Color.
#[uniffi::remote(Record)]
pub struct TagEntry {
    pub name: String,
    /// `#rrggbb`; none is Default.
    pub color: Option<String>,
}

/// What a Tag field offers for the typed text.
#[uniffi::remote(Record)]
pub struct TagSuggestions {
    /// Known Tags: prefix matches, then matches anywhere, each sorted.
    pub matches: Vec<String>,
    /// Whether to offer "Create Tag" for the typed text.
    pub create: bool,
}

/// A Task as the lists show it, one row of a flat list, see
/// `pkm_tasks::Task` (named apart from Swift's `Task`).
#[derive(uniffi::Record)]
pub struct ListedTask {
    pub id: String,
    pub title: String,
    /// 1 (highest) to 4.
    pub priority: u8,
    /// Plain text; empty if none.
    pub description: String,
    /// In the order they were added, each with its Tag Color.
    pub tags: Vec<TagEntry>,
    /// Closed as Done, the Close still pending: shown in place, marked.
    pub done: bool,
    /// The Parent's ID, if it is a Subtask.
    pub parent: Option<String>,
    /// The Parent's Title, if it is a Subtask. Only listings fill this and
    /// the Subtask counts; a Task a change returns has none.
    pub parent_title: Option<String>,
    /// How many of its direct Subtasks are still Open.
    pub open_subtasks: u32,
    /// Whether any Task it's Blocked by is Open.
    pub blocked: bool,
    /// The Tasks it's Blocked by, Open or Closed.
    pub blocked_by: Vec<Blocker>,
}

impl ListedTask {
    /// `t` with each of its Tags in the Tag Color `known` gives it.
    fn new(t: Task, known: &Tags) -> Self {
        let tags = t.tags.into_iter().map(|name| TagEntry {
            color: known.color(&name).map(str::to_owned),
            name,
        });
        Self {
            id: t.id,
            title: t.title,
            priority: t.priority,
            description: t.description,
            tags: tags.collect(),
            done: t.done,
            parent: t.parent,
            parent_title: None,
            open_subtasks: 0,
            blocked: t.blocked,
            blocked_by: t.blocked_by,
        }
    }

    fn of_row(row: Row, known: &Tags) -> Self {
        Self {
            parent_title: row.parent_title,
            open_subtasks: row.open_subtasks,
            ..Self::new(row.task, known)
        }
    }
}

/// What a Shortcut sets.
#[uniffi::remote(Enum)]
pub enum ShortcutMeaning {
    Priority(u8),
    Tag(String),
}

/// A Shortcut in a Quick Add text, at `start..end` in UTF-16 code units.
#[uniffi::remote(Record)]
pub struct QuickShortcut {
    pub start: u32,
    pub end: u32,
    pub meaning: ShortcutMeaning,
}

/// What to suggest while the cursor is in an `@` word.
#[uniffi::remote(Record)]
pub struct TagQuery {
    pub query: String,
    pub leave_out: Vec<String>,
}

/// The text of a Title field with Shortcuts in it, see `pkm_tasks::QuickAdd`.
/// One per field; the object is shared, so it locks.
#[derive(uniffi::Object)]
pub struct TaskQuickAdd(Mutex<QuickAdd>);

#[uniffi::export]
impl TaskQuickAdd {
    /// An empty text, for a new Task.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self(Mutex::new(QuickAdd::new()))
    }

    /// The text of a saved Title, for editing, with every Shortcut-like part
    /// Ignored.
    #[uniffi::constructor]
    pub fn from_title(title: String) -> Self {
        Self(Mutex::new(QuickAdd::from_title(&title)))
    }

    /// Replaces the UTF-16 range `start..end` with `with` and moves the cursor
    /// behind it.
    pub fn replace(&self, start: u32, end: u32, with: String) {
        self.lock().replace(start, end, &with);
    }

    /// Ignores the Shortcut at a UTF-16 position, keeping it as plain text.
    pub fn ignore(&self, at: u32) {
        self.lock().ignore(at);
    }

    /// Ignores every Priority Shortcut, as when the Priority is picked by hand.
    pub fn ignore_priority(&self) {
        self.lock().ignore_priority();
    }

    /// Puts the cursor at a UTF-16 position.
    pub fn move_cursor(&self, at: u32) {
        self.lock().move_cursor(at);
    }

    /// Where the cursor is, in UTF-16 code units.
    pub fn cursor(&self) -> u32 {
        self.lock().cursor()
    }

    /// The text as typed.
    pub fn text(&self) -> String {
        self.lock().text()
    }

    /// The Shortcuts to draw highlighted.
    pub fn highlights(&self) -> Vec<QuickShortcut> {
        self.lock().highlights()
    }

    /// The Title to save; empty if only Shortcuts were typed.
    pub fn title(&self) -> String {
        self.lock().title()
    }

    /// The Priority to apply, if a Shortcut sets one.
    pub fn priority(&self) -> Option<u8> {
        self.lock().priority()
    }

    /// The Tags to add.
    pub fn tags(&self) -> Vec<String> {
        self.lock().tags()
    }

    /// What to suggest, if the cursor is in an `@` word.
    pub fn tag_query(&self) -> Option<TagQuery> {
        self.lock().tag_query()
    }

    /// Completes the `@` word under the cursor to `@tag `, cursor behind it.
    pub fn pick_tag(&self, tag: String) {
        self.lock().pick_tag(&tag);
    }
}

impl TaskQuickAdd {
    fn lock(&self) -> std::sync::MutexGuard<'_, QuickAdd> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// The Tasks in a folder, see `pkm_tasks::Tasks`.
#[derive(uniffi::Object)]
pub struct TaskStore(Tasks);

impl TaskStore {
    /// `task` as the lists show it.
    fn listed(&self, task: Task) -> ListedTask {
        ListedTask::new(task, &self.0.tags())
    }

    /// `rows` as the lists show them.
    fn listed_rows(&self, rows: Vec<Row>) -> Vec<ListedTask> {
        let known = self.0.tags();
        rows.into_iter()
            .map(|row| ListedTask::of_row(row, &known))
            .collect()
    }
}

#[uniffi::export]
impl TaskStore {
    /// Opens the Tasks in the folder `root`, creating it if needed.
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<Self, TasksError> {
        Ok(Self(Tasks::open(root)?))
    }

    /// Moves a Subtask before its sibling `before`, or after its last sibling;
    /// its Parent stays. See `pkm_tasks::Tasks::move_subtask`.
    pub fn move_subtask(
        &self,
        id: String,
        before: Option<String>,
    ) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.move_subtask(&id, before.as_deref())?))
    }

    /// Closes a Task as a pending change, committing the previous one first.
    /// Nothing is Deleted until `commit`; `revert` takes it back.
    pub fn close(&self, id: String, closing: Closing) -> Result<(), TasksError> {
        Ok(self.0.close(&id, closing)?)
    }

    /// Drops the pending Close, leaving no trace.
    pub fn revert(&self) {
        self.0.revert();
    }

    /// Deletes the files of the pending Close, if any.
    pub fn commit(&self) -> Result<(), TasksError> {
        Ok(self.0.commit()?)
    }

    /// Creates a Task with a Priority and Tags, refusing an empty Title or an
    /// invalid Tag. Created in the Filter `filter`, it also gets the Tags its
    /// Query requires, so that it stays there.
    pub fn create(
        &self,
        title: String,
        priority: u8,
        tags: Vec<String>,
        filter: Option<String>,
    ) -> Result<ListedTask, TasksError> {
        let task = self.0.create(&title, priority, &tags, filter.as_deref())?;
        Ok(self.listed(task))
    }

    /// `create` for a Subtask under a Task; it starts with its Parent's Tags.
    pub fn create_subtask(
        &self,
        parent: String,
        title: String,
        priority: u8,
        tags: Vec<String>,
    ) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.create_subtask(&parent, &title, priority, &tags)?))
    }

    /// Sets a Task's Title, Description and Priority and adds Tags in one
    /// write, refusing an empty Title or an invalid Tag.
    pub fn edit(
        &self,
        id: String,
        title: String,
        description: String,
        priority: u8,
        add_tags: Vec<String>,
    ) -> Result<ListedTask, TasksError> {
        Ok(self.listed(
            self.0
                .edit(&id, &title, &description, priority, &add_tags)?,
        ))
    }

    /// Adds a Tag to a Task, refusing an invalid one.
    pub fn add_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.add_tag(&id, &tag)?))
    }

    /// Removes a Tag from a Task.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.remove_tag(&id, &tag)?))
    }

    /// Marks a Task as Blocked by another Open Task; refused for itself, its
    /// Parent, its Subtask and a loop.
    pub fn add_blocker(&self, id: String, blocker: String) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.add_blocker(&id, &blocker)?))
    }

    /// The Open Tasks of both lists a Task could be Blocked by: the ones
    /// `add_blocker` accepts, minus its current Blockers.
    pub fn blocker_candidates(&self, id: String) -> Result<Vec<ListedTask>, TasksError> {
        Ok(self.listed_rows(self.0.blocker_candidates(&id)?))
    }

    /// Removes a Blocked by from a Task.
    pub fn remove_blocker(&self, id: String, blocker: String) -> Result<ListedTask, TasksError> {
        Ok(self.listed(self.0.remove_blocker(&id, &blocker)?))
    }

    /// What a Tag field offers for the typed `query`, leaving out the Tags the
    /// Task already has: known Tags starting with it, then containing it, and
    /// whether to offer Create.
    pub fn suggest_tags(
        &self,
        query: String,
        leave_out: Vec<String>,
    ) -> Result<TagSuggestions, TasksError> {
        Ok(self.0.suggest_tags(&query, &leave_out)?)
    }

    /// Gives a Tag its Tag Color (`#rrggbb`), or Default for none.
    pub fn set_tag_color(&self, tag: String, color: Option<String>) -> Result<(), TasksError> {
        Ok(self.0.set_tag_color(&tag, color.as_deref())?)
    }

    /// Deletes a Tag: Removes it from every Item, Open or Closed, then forgets
    /// it and its Tag Color. Removing first means an interrupted Delete can
    /// just be repeated.
    pub fn delete_tag(&self, tag: String) -> Result<(), TasksError> {
        self.0.remove_tag_everywhere(&tag)?;
        Ok(self.0.forget_tag(&tag)?)
    }

    /// Every known Tag with its Tag Color, alphabetically.
    pub fn tags(&self) -> Vec<TagEntry> {
        self.0.tags().list()
    }

    /// Every Open Task, Subtasks included, in the order of All Tasks.
    pub fn tasks(&self) -> Result<Vec<ListedTask>, TasksError> {
        Ok(self.listed_rows(self.0.rows()?))
    }

    /// Creates a Filter at the end of the Filters list, refusing an empty
    /// Title or a Query that can't be read.
    pub fn create_filter(&self, title: String, query: String) -> Result<Filter, TasksError> {
        Ok(self.0.create_filter(&title, &query)?)
    }

    /// Changes a Filter's Title and Query, refusing what `create_filter` does.
    pub fn edit_filter(
        &self,
        id: String,
        title: String,
        query: String,
    ) -> Result<Filter, TasksError> {
        Ok(self.0.edit_filter(&id, &title, &query)?)
    }

    /// Marks a Filter as a Favorite, or unmarks it.
    pub fn set_favorite(&self, id: String, favorite: bool) -> Result<(), TasksError> {
        Ok(self.0.set_favorite(&id, favorite)?)
    }

    /// Moves a Filter before another, or to the end of the Filters list.
    pub fn move_filter(&self, id: String, before: Option<String>) -> Result<(), TasksError> {
        Ok(self.0.move_filter(&id, before.as_deref())?)
    }

    /// Deletes a Filter; no Task changes.
    pub fn delete_filter(&self, id: String) -> Result<(), TasksError> {
        Ok(self.0.delete_filter(&id)?)
    }

    /// Every Filter with its match count, in the Filters list's order.
    pub fn filters(&self) -> Result<Vec<Filter>, TasksError> {
        Ok(self.0.filters()?)
    }

    /// The Open Tasks a Filter matches, in its order.
    pub fn filter_tasks(&self, id: String) -> Result<Vec<ListedTask>, TasksError> {
        Ok(self.listed_rows(self.0.filter_rows(&id)?))
    }

    /// Moves a Task before another in a Filter's own order, or to its end.
    pub fn move_in_filter(
        &self,
        id: String,
        task: String,
        before: Option<String>,
    ) -> Result<(), TasksError> {
        Ok(self.0.move_in_filter(&id, &task, before.as_deref())?)
    }

    /// Sorts a Filter; Manual keeps the order shown, Priority from Manual
    /// discards it. No Task changes.
    pub fn set_sort(&self, id: String, sort: FilterSort) -> Result<(), TasksError> {
        Ok(self.0.set_sort(&id, sort)?)
    }

    /// Checks a Query text without saving: what is wrong with it, or which of
    /// its Tags don't exist.
    pub fn check_query(&self, text: String) -> QueryCheck {
        match self.0.check_query(&text) {
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
    pub fn subtasks(&self, parent: String) -> Result<Vec<ListedTask>, TasksError> {
        Ok(self.listed_rows(self.0.subtasks(&parent)?))
    }
}

/// `text` as a Query may be typed: lowercase, without what neither a Tag nor
/// the Query syntax can hold.
#[uniffi::export]
pub fn typed_query(text: String) -> String {
    pkm_tasks::typed_query(&text)
}

/// The Tag being typed at the end of a Query `text`, to suggest Tags for; none
/// if it doesn't end in one.
#[uniffi::export]
pub fn unfinished_query_tag(text: String) -> Option<String> {
    pkm_tasks::unfinished_tag(&text).map(str::to_owned)
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
        let store = TaskStore::open(path("Tasks")).unwrap();
        let task = store.create("t".into(), 4, vec![], None).unwrap();
        store.add_tag(task.id.clone(), "home".into()).unwrap();
        let task = store.add_tag(task.id, "work".into()).unwrap();
        assert_eq!(colors(&task), [("home", None), ("work", None)]);

        store
            .set_tag_color("work".into(), Some("#d0021b".into()))
            .unwrap();
        let listed = store.tasks().unwrap();
        assert_eq!(
            colors(&listed[0]),
            [("home", None), ("work", Some("#d0021b"))]
        );

        store.set_tag_color("work".into(), None).unwrap();
        assert_eq!(colors(&store.tasks().unwrap()[0])[1], ("work", None));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn deleting_a_tag_removes_it_from_every_task_and_forgets_it() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-del-{}", std::process::id()));
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let store = TaskStore::open(path("Tasks")).unwrap();
        let task = store.create("t".into(), 4, vec![], None).unwrap();
        store.add_tag(task.id.clone(), "home".into()).unwrap();
        store.add_tag(task.id, "work".into()).unwrap();
        store
            .set_tag_color("work".into(), Some("#d0021b".into()))
            .unwrap();

        store.delete_tag("work".into()).unwrap();

        let listed = store.tasks().unwrap();
        assert_eq!(colors(&listed[0]), [("home", None)]);
        let names: Vec<_> = store.tags().into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["home"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_filter_lists_its_tasks_as_listed_tasks_and_a_query_is_checked() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-filter-{}", std::process::id()));
        let path = |name: &str| dir.join(name).to_string_lossy().into_owned();
        let store = TaskStore::open(path("Tasks")).unwrap();
        for favorite in store.filters().unwrap() {
            store.delete_filter(favorite.id).unwrap(); // the migration's
        }
        store
            .create("t".into(), 4, vec!["work".into()], None)
            .unwrap();
        store.create("u".into(), 4, vec![], None).unwrap();
        let filter = store.create_filter("Work".into(), "@work".into()).unwrap();
        let tasks = store.filter_tasks(filter.id.clone()).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "t");
        assert!(matches!(
            store.create_filter("Work".into(), "@work &".into()),
            Err(TasksError::InvalidQuery(_))
        ));
        let other = store.create_filter("Other".into(), "@x".into()).unwrap();
        let edited = store
            .edit_filter(other.id.clone(), "Home".into(), "@work".into())
            .unwrap();
        assert_eq!(edited.matches, 1);
        store.move_filter(other.id.clone(), None).unwrap();
        store
            .move_filter(other.id.clone(), Some(filter.id.clone()))
            .unwrap();
        assert_eq!(store.filters().unwrap()[0].title, "Home");
        store.set_favorite(other.id.clone(), true).unwrap();
        assert!(store.filters().unwrap()[0].favorite);
        let first = store
            .create("a".into(), 4, vec!["work".into()], None)
            .unwrap();
        store
            .create("b".into(), 4, vec!["work".into()], None)
            .unwrap();
        store
            .move_in_filter(filter.id.clone(), first.id.clone(), None)
            .unwrap();
        let order: Vec<String> = (store.filter_tasks(filter.id.clone()).unwrap().into_iter())
            .map(|t| t.title)
            .collect();
        assert_eq!(order.last().map(String::as_str), Some("a"));
        store.delete_filter(other.id).unwrap();
        assert_eq!(store.filters().unwrap().len(), 1);
        assert_eq!(store.tasks().unwrap().len(), 4);

        let problem = store.check_query("@a & (".into()).problem.unwrap();
        assert_eq!(
            (problem.position, problem.message.as_str()),
            (6, "Expected @tag, ! or (")
        );
        let checked = store.check_query("@work | !@b".into());
        assert!(checked.problem.is_none());
        assert_eq!(checked.unknown, ["b"]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
