//! The Tasks store: a folder of `<id>.md` files.

use crate::history::{Event, History};
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::{Document, Value};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use pkm_base::tag;
use std::cmp::Ordering;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const DEFAULT_PRIORITY: u8 = 4;

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

/// An Open Task as the lists show it.
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
}

#[derive(Debug)]
pub enum Error {
    /// The Title is empty or only whitespace.
    EmptyTitle,
    /// A Tag with a forbidden character, or empty, or a misplaced `/`.
    InvalidTag,
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
            Self::Io(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for Error {}

/// The Tasks in one folder (`Tasks` in the app's storage).
#[derive(Debug)]
pub struct Tasks {
    root: PathBuf,
    history: History,
}

impl Tasks {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Tasks";

    /// Opens the Tasks in `root`, creating the folder if needed. `device` is
    /// this Device's ID, which names its History files in `root/.history`.
    pub fn open(root: impl Into<PathBuf>, device: ItemId) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        let history = History::new(root.join(".history"), device);
        Ok(Self { root, history })
    }

    /// Creates a Task at the end of its list, with priority 4. The Title is
    /// trimmed and kept to one line (line breaks become spaces).
    pub fn create(&self, title: &str, planning: Planning) -> Result<Task, Error> {
        let title = one_line(title);
        if title.is_empty() {
            return Err(Error::EmptyTitle);
        }
        let order = self
            .list(planning)?
            .last()
            .map_or(1.0, |last| last.order + 1.0);
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", &title);
            file.set_scalar("priority", DEFAULT_PRIORITY.to_string());
            file.set_scalar("state", planning.state());
            file.set_scalar("order", order.to_string());
            file.set_scalar("updated", iso8601(now_ms()));
            match self.write(&id, &file.render(), Overwrite::No) {
                Ok(()) => {
                    self.history.record(Event::Created, &id, now_ms())?;
                    return Ok(Task {
                        id: id.to_string(),
                        title,
                        priority: DEFAULT_PRIORITY,
                        planning,
                        description: String::new(),
                        tags: Vec::new(),
                    });
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
        let title = one_line(title);
        if title.is_empty() {
            return Err(Error::EmptyTitle);
        }
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

    /// Plans the Open Task `id` as `planning`, at the end of that list, and
    /// records it in the History. A Task already planned so stays untouched.
    /// Fails with `NotFound` if there is no such Task.
    pub fn plan(&self, id: &str, planning: Planning) -> Result<Task, Error> {
        let item = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let (_, current) = self.read_open(&item)?;
        if current.planning == planning {
            return Ok(current);
        }
        let order = self
            .list(planning)?
            .last()
            .map_or(1.0, |last| last.order + 1.0);
        let task = self.modify(id, |file| {
            file.set_scalar("state", planning.state());
            file.set_scalar("order", order.to_string());
        })?;
        let event = match planning {
            Planning::Active => Event::MadeActive,
            Planning::Backlog => Event::MovedToBacklog,
        };
        self.history.record(event, &item, now_ms())?;
        Ok(task)
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
                set_tags(file, tags);
            }
        })
    }

    /// Removes `tag` from the Open Task `id`; a Tag it doesn't have is
    /// ignored. Fails with `NotFound` if there is no such Task.
    pub fn remove_tag(&self, id: &str, tag: &str) -> Result<Task, Error> {
        self.modify(id, |file| {
            let mut tags = tags_of(file);
            tags.retain(|t| t != tag);
            set_tags(file, tags);
        })
    }

    /// The Tags that start with `prefix` and are carried by an Open or Closed
    /// Task, each once, sorted.
    pub fn tags_with_prefix(&self, prefix: &str) -> io::Result<Vec<String>> {
        let mut found = Vec::new();
        for item in fs::read_dir(&self.root)? {
            let path = item?.path();
            if path.extension().is_some_and(|e| e == "md")
                && let Ok(text) = fs::read_to_string(&path)
                && let Some(file) = Document::parse(&text)
            {
                found.extend(tags_of(&file).into_iter().filter(|t| t.starts_with(prefix)));
            }
        }
        found.sort();
        found.dedup();
        Ok(found)
    }

    /// The file of the Open Task `id` and the Task it describes.
    fn read_open(&self, id: &ItemId) -> Result<(Document, Task), Error> {
        let file = Document::parse(&fs::read_to_string(self.path(id))?)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unreadable Task"))?;
        let task = task_of(&file).ok_or(io::ErrorKind::NotFound)?;
        Ok((file, task))
    }

    /// Applies `change` to the Open Task `id`, bumps `updated` and saves.
    /// Every property `change` leaves alone, known or not, stays as it was.
    fn modify(&self, id: &str, change: impl FnOnce(&mut Document)) -> Result<Task, Error> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let (mut file, _) = self.read_open(&id)?;
        change(&mut file);
        file.set_scalar("updated", iso8601(now_ms()));
        self.write(&id, &file.render(), Overwrite::Yes)?;
        Ok(task_of(&file).ok_or(io::ErrorKind::InvalidData)?)
    }

    /// The Tasks planned as `planning`, in their order.
    pub fn tasks(&self, planning: Planning) -> io::Result<Vec<Task>> {
        Ok(self.list(planning)?.into_iter().map(|e| e.task).collect())
    }

    fn list(&self, planning: Planning) -> io::Result<Vec<Entry>> {
        let mut entries = Vec::new();
        for item in fs::read_dir(&self.root)? {
            let path = item?.path();
            if path.extension().is_some_and(|e| e == "md")
                && let Some(entry) = read(&path, planning)
            {
                entries.push(entry);
            }
        }
        entries.sort_by(|a, b| {
            a.order
                .partial_cmp(&b.order)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.task.id.cmp(&b.task.id))
        });
        Ok(entries)
    }

    fn path(&self, id: &ItemId) -> PathBuf {
        self.root.join(format!("{id}.md"))
    }

    /// Writes `<id>.md` through a hidden scratch file, so a crash leaves no
    /// torn Task and Sync never sees half of one. Unless told to overwrite,
    /// fails with `AlreadyExists` rather than replacing a Task.
    fn write(&self, id: &ItemId, text: &str, overwrite: Overwrite) -> io::Result<()> {
        let path = self.path(id);
        let scratch = self.root.join(format!(".{id}.tmp"));
        if overwrite == Overwrite::No && path.exists() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        let written = fs::File::create(&scratch).and_then(|mut f| f.write_all(text.as_bytes()));
        written
            .and_then(|()| fs::rename(&scratch, &path))
            .inspect_err(|_| drop(fs::remove_file(&scratch)))
    }
}

#[derive(PartialEq, Eq)]
enum Overwrite {
    Yes,
    No,
}

struct Entry {
    task: Task,
    order: f64,
}

/// The Task in `path`, if it is one planned as `planning`.
fn read(path: &Path, planning: Planning) -> Option<Entry> {
    let file = Document::parse(&fs::read_to_string(path).ok()?)?;
    let task = task_of(&file).filter(|t| t.planning == planning)?;
    let order = file
        .scalar("order")
        .and_then(|o| o.parse().ok())
        .unwrap_or(0.0);
    Some(Entry { task, order })
}

/// The Open Task a file describes; `None` for a closed or malformed one.
fn task_of(file: &Document) -> Option<Task> {
    let planning = match file.scalar("state")? {
        "active" => Planning::Active,
        "backlog" => Planning::Backlog,
        _ => return None,
    };
    Some(Task {
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
    })
}

/// The valid Tags in a file; others, which only other tools could have
/// written, are ignored.
fn tags_of(file: &Document) -> Vec<String> {
    let tags = file.list("tags").unwrap_or_default();
    tags.iter().filter(|t| tag::is_valid(t)).cloned().collect()
}

/// An empty list is no property at all.
fn set_tags(file: &mut Document, tags: Vec<String>) {
    if tags.is_empty() {
        file.remove("tags");
    } else {
        file.set("tags", Value::List(tags));
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
            Tasks::open(&self.0, ItemId::parse("dddddddddddd").unwrap()).unwrap()
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
    fn a_tag_only_closed_tasks_carry_is_still_listed_but_an_unused_one_is_not() {
        let dir = Scratch::new();
        let tasks = dir.store();
        let task = tagged(&tasks, &["gone", "closed"]);
        tasks.remove_tag(&task.id, "gone").unwrap();
        let path = dir.0.join(format!("{}.md", task.id));
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("state: active", "state: done");
        fs::write(&path, text).unwrap();
        assert_eq!(tasks.tags_with_prefix("").unwrap(), ["closed"]);
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
}
