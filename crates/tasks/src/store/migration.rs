//! The migration of ADR-0025, run once per Device when the store opens: Active
//! and the Backlog stop being states of the Task files and become Tags and
//! Favorites.
//!
//! - A Task in the Backlog (or under one) gets the Tag `backlog`; `state`,
//!   `position` and `order` leave its file, `created` joins it (from the
//!   History, else the file's modification time), `updated` stays. A Subtask
//!   gets its `subtask-position` numbered from its siblings' old order. Closed
//!   Tasks' files are Deleted, then the History.
//! - All Tasks, Active and Backlog are created as Favorites with fixed IDs, so
//!   that Devices migrating before they Sync end with one set. Their places
//!   come from the old lists' orders. They are stamped as never `updated`, so
//!   that a Device creating them anew (a new install) loses every merge
//!   against an edit made elsewhere.
//! - Each other Filter keeps showing what it showed, in the order it showed
//!   it: `& !@backlog` joins its Query and its order becomes its places.
//!
//! The Filters are written first and the marker last, so that an interrupted
//! run starts over from the files that are still legacy.

use super::filters::{Stored, in_filter_order, matching, set_places};
use super::{
    CREATED, Node, Nodes, Overwrite, SUBTASK_POSITION, Tasks, end_key, modified, node_of, repair,
};
use super::{set_list, tags_of};
use crate::query::Query;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::Document;
use pkm_base::item_id::ItemId;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

/// A Filter the migration creates.
pub(crate) struct Favorite {
    pub(crate) id: &'static str,
    title: &'static str,
    pub(crate) query: &'static str,
}

pub(crate) const ALL_TASKS: Favorite = Favorite {
    id: "tasksall2222",
    title: "All Tasks",
    query: "*",
};
pub(crate) const ACTIVE: Favorite = Favorite {
    id: "tasksactive2",
    title: "Active",
    query: "!@backlog",
};
pub(crate) const BACKLOG_LIST: Favorite = Favorite {
    id: "tasksbacklog",
    title: "Backlog",
    query: "@backlog",
};

/// The Tag a Task in the Backlog gets.
const BACKLOG: &str = "backlog";

/// What a Query gets to keep an existing Filter from showing the Backlog.
const NOT_BACKLOG: &str = " & !@backlog";

/// The `updated` of a Favorite the migration creates: older than any edit.
const NEVER_UPDATED: &str = "1970-01-01T00:00:00Z";

/// Where old versions kept the History (ADR-0022): `<device>-<yyyy-mm>.log`
/// files of `<time> <event> <id>` lines, from every Device.
fn history(root: &Path) -> PathBuf {
    root.join(".history")
}

/// Whose presence says this Device migrated. A hidden file in the Tasks
/// folder: Sync leaves it on the Device.
pub(super) fn marker(root: &Path) -> PathBuf {
    root.join(Tasks::MIGRATED)
}

/// A Task file of an old version.
struct Legacy {
    doc: Document,
    node: Node,
    /// Planned in the Backlog, by itself or through its top-level Task.
    backlog: bool,
    /// Where it was in its list, by `position`; none for a file that has only `order`.
    position: Option<f64>,
    order: f64,
}

impl Tasks {
    /// Migrates this Device's files, unless that was done; see the module.
    pub(super) fn migrate_lists(&self) -> io::Result<()> {
        if self.files.exists(&marker(&self.root)) {
            return Ok(());
        }
        let (open, closed) = self.read_legacy()?;
        let [active, backlog] = list_orders(&open);
        self.migrate_filters(&active, &backlog)?;
        let subtask_keys = subtask_keys(&[&active, &backlog]);
        for task in &open {
            let key = subtask_keys.get(task.node.task.id.as_str());
            self.migrate_task(task, key.copied())?;
        }
        let logs = self.files.list(&history(&self.root)).unwrap_or_default();
        let paths = (closed.iter().map(|id| self.path(id)))
            .chain(logs.into_iter().map(|l| history(&self.root).join(l.name)));
        for path in paths {
            match self.files.remove(&path) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        self.files.write(&marker(&self.root), "")
    }

    /// When each Task was created, by Item ID, from the History's `created`
    /// lines; the earliest wins.
    fn created_in_history(&self) -> HashMap<String, String> {
        let folder = history(&self.root);
        let mut times: HashMap<String, String> = HashMap::new();
        let logs = self.files.list(&folder).unwrap_or_default();
        for text in logs
            .iter()
            .filter_map(|l| self.files.read(&folder.join(&l.name)).ok())
        {
            for line in text.lines() {
                if let [time, "created", id] = line.split(' ').collect::<Vec<_>>()[..]
                    && times.get(id).is_none_or(|t| time < t.as_str())
                {
                    times.insert(id.to_owned(), time.to_owned());
                }
            }
        }
        times
    }

    /// The Open Task files of an old version (Subtasks planned as their
    /// top-level Task is), and the IDs of the Closed ones.
    fn read_legacy(&self) -> io::Result<(Vec<Legacy>, Vec<ItemId>)> {
        let mut docs = Vec::new();
        let mut closed = Vec::new();
        let logged = self.created_in_history();
        for listed in self.files.list(&self.root)? {
            let Some(id) = listed.name.strip_suffix(".md").and_then(ItemId::parse) else {
                continue;
            };
            let Some(doc) =
                (self.files.read(&self.path(&id)).ok()).and_then(|t| Document::parse(&t))
            else {
                continue;
            };
            match doc.scalar("state") {
                Some("done" | "rejected") if doc.scalar("id") == Some(id.as_str()) => {
                    closed.push(id);
                }
                Some("active" | "backlog") => {
                    let Some(mut node) = node_of(&doc).filter(|n| n.task.id == id.as_str()) else {
                        continue;
                    };
                    if node.created.is_none() {
                        let created = logged.get(id.as_str()).cloned();
                        node.created = created.or_else(|| Some(modified(listed.stamp)));
                    }
                    docs.push((doc, node));
                }
                _ => {}
            }
        }
        let mut nodes: Nodes = docs
            .iter()
            .map(|(_, n)| (n.task.id.clone(), n.clone()))
            .collect();
        repair(&mut nodes);
        let in_backlog: HashMap<&str, bool> = docs
            .iter()
            .map(|(doc, n)| (n.task.id.as_str(), doc.scalar("state") == Some("backlog")))
            .collect();
        let open = docs
            .iter()
            .map(|(doc, node)| {
                let id = node.task.id.as_str();
                let mut root = id;
                while let Some(parent) = nodes[root].task.parent.as_deref() {
                    root = parent;
                }
                Legacy {
                    doc: doc.clone(),
                    node: Node {
                        task: nodes[id].task.clone(),
                        ..node.clone()
                    },
                    backlog: in_backlog[root],
                    position: doc.scalar("position").and_then(|p| p.parse().ok()),
                    order: doc
                        .scalar("order")
                        .and_then(|o| o.parse().ok())
                        .unwrap_or(0.0),
                }
            })
            .collect();
        Ok((open, closed))
    }

    /// Creates the three Favorites and rewrites every other Filter.
    fn migrate_filters(&self, active: &[&Legacy], backlog: &[&Legacy]) -> io::Result<()> {
        let stored = self.stored_filters()?;
        // The Tasks the old Filters looked at, in Active's order.
        let seen: Vec<&Node> = active.iter().map(|l| &l.node).collect();
        let fixed = [&ALL_TASKS.id, &ACTIVE.id, &BACKLOG_LIST.id];
        for filter in stored
            .iter()
            .filter(|s| !fixed.contains(&&s.filter.id.as_str()))
        {
            self.migrate_filter(filter, &seen)?;
        }
        let ids = |list: &[&Legacy]| -> Vec<String> {
            list.iter().map(|l| l.node.task.id.clone()).collect()
        };
        let (active, backlog) = (ids(active), ids(backlog));
        let both: Vec<String> = active.iter().chain(&backlog).cloned().collect();
        let mut position = end_key(&stored.iter().map(|s| s.position).collect::<Vec<_>>());
        for (favorite, order) in [(ALL_TASKS, both), (ACTIVE, active), (BACKLOG_LIST, backlog)] {
            if !self
                .files
                .exists(&self.filter_path(&filter_id(favorite.id)))
            {
                self.create_favorite(&favorite, position, &order)?;
                position += 1.0;
            }
        }
        Ok(())
    }

    fn create_favorite(
        &self,
        favorite: &Favorite,
        position: f64,
        order: &[String],
    ) -> io::Result<()> {
        let mut file = Document::default();
        file.set_scalar("id", favorite.id);
        file.set_scalar("title", favorite.title);
        file.set_scalar("query", favorite.query);
        file.set_scalar("favorite", "true");
        file.set_scalar("position", position.to_string());
        set_places(&mut file, &numbered(order));
        file.set_scalar("updated", NEVER_UPDATED);
        self.files
            .write(&self.filter_path(&filter_id(favorite.id)), &file.render())
    }

    /// Keeps `filter` showing the Tasks of `seen` it showed, in its order: the
    /// Backlog joins nothing, and the order shown becomes its places. One that
    /// can't be read, or already excludes the Backlog, stays as it is.
    fn migrate_filter(&self, filter: &Stored, seen: &[&Node]) -> io::Result<()> {
        let query = &filter.filter.query;
        let excludes_backlog =
            query.starts_with('(') && query.ends_with(&format!("){NOT_BACKLOG}"));
        if excludes_backlog || Query::parse(query).is_err() {
            return Ok(());
        }
        let shown = in_filter_order(matching(seen.to_vec(), query), &filter.places);
        let path = self.filter_path(&filter_id(&filter.filter.id));
        let mut file =
            Document::parse(&self.files.read(&path)?).ok_or(io::ErrorKind::InvalidData)?;
        file.set_scalar("query", format!("({query}){NOT_BACKLOG}"));
        let order: Vec<String> = shown.iter().map(|n| n.task.id.clone()).collect();
        set_places(&mut file, &numbered(&order));
        file.set_scalar("updated", iso8601(now_ms()));
        self.files.write(&path, &file.render())
    }

    /// Rewrites one Task file as the new format has it, a Subtask with its
    /// `subtask_key`; `updated` stays.
    fn migrate_task(&self, task: &Legacy, subtask_key: Option<f64>) -> io::Result<()> {
        let mut file = task.doc.clone();
        for key in ["state", "position", "order"] {
            file.remove(key);
        }
        if let Some(created) = &task.node.created
            && file.scalar(CREATED).is_none()
        {
            file.set_scalar(CREATED, created);
        }
        if let Some(key) = subtask_key {
            file.set_scalar(SUBTASK_POSITION, key.to_string());
        }
        let mut tags = tags_of(&file);
        if task.backlog && !tags.iter().any(|t| t == BACKLOG) {
            tags.push(BACKLOG.to_owned());
            set_list(&mut file, "tags", tags);
        }
        let id = ItemId::parse(&task.node.task.id).ok_or(io::ErrorKind::InvalidData)?;
        self.write(&id, &file, Overwrite::Yes)
    }
}

fn filter_id(id: &str) -> ItemId {
    ItemId::parse(id).expect("a fixed or stored Filter ID is well formed")
}

/// Places 1, 2, 3 ... in the order of `ids`.
fn numbered(ids: &[String]) -> Vec<(String, f64)> {
    (ids.iter().enumerate())
        .map(|(n, id)| (id.clone(), (n + 1) as f64))
        .collect()
}

/// The `subtask-position` of each Subtask, by Item ID: 1, 2, 3 ... among its
/// siblings in the order of the old `lists`. A Parent's Subtasks are all in
/// the Parent's list.
fn subtask_keys<'a>(lists: &[&[&'a Legacy]]) -> HashMap<&'a str, f64> {
    let mut counts: HashMap<&str, f64> = HashMap::new();
    let mut keys = HashMap::new();
    for task in lists.iter().copied().flatten() {
        if let Some(parent) = task.node.task.parent.as_deref() {
            let count = counts.entry(parent).or_default();
            *count += 1.0;
            keys.insert(task.node.task.id.as_str(), *count);
        }
    }
    keys
}

/// The Active and the Backlog list of an old version, each in its order.
///
/// A file without `position` gets one as the first migration (ADR-0021) did:
/// in the tree order of `order` (top-level Tasks by it, each followed by its
/// Subtasks, depth first), after the Tasks that have one. It is deterministic,
/// so Devices migrating at once agree.
fn list_orders(tasks: &[Legacy]) -> [Vec<&Legacy>; 2] {
    let mut place: HashMap<&str, f64> = HashMap::new();
    for backlog in [false, true] {
        let list: Vec<&Legacy> = tasks.iter().filter(|t| t.backlog == backlog).collect();
        let key = |t: &Legacy| t.position.unwrap_or(t.order);
        let mut last = (list.iter()).filter_map(|t| t.position).fold(0.0, f64::max);
        let mut children: HashMap<Option<&str>, Vec<&Legacy>> = HashMap::new();
        for task in &list {
            children
                .entry(task.node.task.parent.as_deref())
                .or_default()
                .push(task);
        }
        for siblings in children.values_mut() {
            siblings.sort_by(|a, b| {
                (key(a)
                    .partial_cmp(&key(b))
                    .unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.node.task.id.cmp(&b.node.task.id))
            });
        }
        let mut todo = children.remove(&None).unwrap_or_default();
        todo.reverse();
        while let Some(task) = todo.pop() {
            let id = task.node.task.id.as_str();
            place.insert(
                id,
                task.position.unwrap_or_else(|| {
                    last += 1.0;
                    last
                }),
            );
            if let Some(subtasks) = children.remove(&Some(id)) {
                todo.extend(subtasks.into_iter().rev());
            }
        }
    }
    let ordered = |backlog: bool| {
        let mut list: Vec<&Legacy> = tasks.iter().filter(|t| t.backlog == backlog).collect();
        list.sort_by(|a, b| {
            let (pa, pb) = (
                place[a.node.task.id.as_str()],
                place[b.node.task.id.as_str()],
            );
            (pa.partial_cmp(&pb).unwrap_or(std::cmp::Ordering::Equal))
                .then_with(|| a.node.task.id.cmp(&b.node.task.id))
        });
        list
    };
    [ordered(false), ordered(true)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Files, MemoryFiles, Row};
    use std::sync::Arc;

    const UPDATED: &str = "2000-01-01T00:00:00Z";

    fn device() -> Arc<MemoryFiles> {
        Arc::new(MemoryFiles::default())
    }

    /// A Task file of an old version. `extra` are more front matter lines.
    fn legacy(files: &MemoryFiles, id: &str, state: &str, extra: &str) {
        let text = format!(
            "---\nid: {id}\ntitle: {id}\npriority: 4\nstate: {state}\n{extra}updated: {UPDATED}\n---\n"
        );
        files.load(Path::new(&format!("Tasks/{id}.md")), text);
    }

    fn filter(files: &MemoryFiles, id: &str, query: &str, places: &[&str]) {
        let places: String = places.iter().map(|p| format!("  - {p}\n")).collect();
        let places = if places.is_empty() {
            String::new()
        } else {
            format!("places:\n{places}")
        };
        let text = format!(
            "---\nid: {id}\ntitle: {id}\nquery: {query}\nfavorite: false\nposition: 1\n{places}updated: {UPDATED}\n---\n"
        );
        files.load(Path::new(&format!("Filters/{id}.md")), text);
    }

    fn file(files: &MemoryFiles, path: &str) -> Document {
        Document::parse(&files.read(Path::new(path)).unwrap()).unwrap()
    }

    fn ids(rows: Vec<Row>) -> Vec<String> {
        rows.into_iter().map(|r| r.task.id).collect()
    }

    /// The Tasks a Filter shows, by the first letter of their IDs.
    fn shown(tasks: &Tasks, filter: &str) -> String {
        let rows = ids(tasks.filter_rows(filter).unwrap());
        rows.iter().map(|id| &id[..1]).collect()
    }

    const TASKS: [(&str, &str, &str); 5] = [
        ("aaaaaaaaaaaa", "active", "position: 2\ntags:\n  - work\n"),
        ("bbbbbbbbbbbb", "active", "position: 1\ntags:\n  - work\n"),
        ("cccccccccccc", "backlog", "position: 1\ntags:\n  - work\n"),
        ("dddddddddddd", "done", "position: 1\n"),
        ("eeeeeeeeeeee", "rejected", "order: 1\n"),
    ];

    fn legacy_device() -> Arc<MemoryFiles> {
        let files = device();
        for (id, state, extra) in TASKS {
            legacy(&files, id, state, extra);
        }
        files
    }

    #[test]
    fn legacy_files_are_rewritten_keeping_updated_and_closed_ones_are_deleted() {
        let files = legacy_device();
        Tasks::open_in(files.clone(), "Tasks");
        let backlog = file(&files, "Tasks/cccccccccccc.md");
        assert_eq!(
            backlog.list("tags"),
            Some(&["work".to_owned(), "backlog".to_owned()][..])
        );
        let active = file(&files, "Tasks/aaaaaaaaaaaa.md");
        assert_eq!(active.list("tags"), Some(&["work".to_owned()][..]));
        for task in [&backlog, &active] {
            assert!(
                ["state", "position", "order"]
                    .iter()
                    .all(|k| task.scalar(k).is_none())
            );
            assert_eq!(task.scalar("updated"), Some(UPDATED));
            assert!(task.scalar("created").is_some());
        }
        for closed in ["dddddddddddd", "eeeeeeeeeeee"] {
            assert!(!files.exists(Path::new(&format!("Tasks/{closed}.md"))));
        }
    }

    #[test]
    fn a_subtask_goes_to_the_backlog_with_its_top_level_task() {
        let files = device();
        legacy(&files, "aaaaaaaaaaaa", "backlog", "");
        legacy(&files, "bbbbbbbbbbbb", "active", "parent: aaaaaaaaaaaa\n");
        legacy(&files, "cccccccccccc", "active", "parent: bbbbbbbbbbbb\n");
        let tasks = Tasks::open_in(files, "Tasks");
        assert_eq!(tasks.rows().unwrap().len(), 3);
    }

    #[test]
    fn the_three_favorites_keep_the_old_orders_all_tasks_showing_active_then_backlog() {
        let files = legacy_device();
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        let listed = tasks.filters().unwrap();
        let summary: Vec<_> = (listed.iter())
            .map(|f| (f.title.as_str(), f.query.as_str(), f.favorite))
            .collect();
        assert_eq!(
            summary,
            [
                ("All Tasks", "*", true),
                ("Active", "!@backlog", true),
                ("Backlog", "@backlog", true)
            ]
        );
        assert_eq!(shown(&tasks, ALL_TASKS.id), "bac");
        assert_eq!(shown(&tasks, ACTIVE.id), "ba");
        assert_eq!(shown(&tasks, BACKLOG_LIST.id), "c");
    }

    #[test]
    fn files_with_only_an_order_list_in_the_tree_order() {
        let files = device();
        legacy(&files, "aaaaaaaaaaaa", "active", "order: 2\n");
        legacy(&files, "bbbbbbbbbbbb", "active", "order: 1\n");
        legacy(&files, "cccccccccccc", "active", "order: 1\n"); // ties with b: by Item ID
        legacy(
            &files,
            "dddddddddddd",
            "active",
            "parent: bbbbbbbbbbbb\norder: 2\n",
        );
        legacy(
            &files,
            "eeeeeeeeeeee",
            "active",
            "parent: bbbbbbbbbbbb\norder: 1\n",
        );
        legacy(
            &files,
            "ffffffffffff",
            "active",
            "parent: eeeeeeeeeeee\norder: 1\n",
        );
        legacy(&files, "gggggggggggg", "active", "position: 9\n");
        let tasks = Tasks::open_in(files, "Tasks");
        assert_eq!(shown(&tasks, ACTIVE.id), "gbefdca");
    }

    #[test]
    fn existing_filters_show_the_same_tasks_in_the_same_order() {
        let files = legacy_device();
        legacy(
            &files,
            "ffffffffffff",
            "active",
            "position: 3\ntags:\n  - work\n",
        );
        // Placed: f, then a; b has no place and so follows. The Backlog Task c isn't shown.
        filter(
            &files,
            "wwwwwwwwwwww",
            "@work",
            &["ffffffffffff 1", "aaaaaaaaaaaa 2"],
        );
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        assert_eq!(shown(&tasks, "wwwwwwwwwwww"), "fab");
        assert_eq!(
            file(&files, "Filters/wwwwwwwwwwww.md").scalar("query"),
            Some("(@work) & !@backlog")
        );
    }

    #[test]
    fn a_filters_unplaced_tasks_keep_actives_order_not_their_creation() {
        let files = device();
        let work = |position, created| {
            format!("position: {position}\ncreated: {created}\ntags:\n  - work\n")
        };
        legacy(
            &files,
            "aaaaaaaaaaaa",
            "active",
            &work(2, "2001-01-01T00:00:00Z"),
        );
        legacy(
            &files,
            "bbbbbbbbbbbb",
            "active",
            &work(1, "2002-01-01T00:00:00Z"),
        );
        filter(&files, "wwwwwwwwwwww", "@work", &[]);
        let tasks = Tasks::open_in(files, "Tasks");
        assert_eq!(shown(&tasks, "wwwwwwwwwwww"), "ba");
    }

    #[test]
    fn created_comes_from_the_history_which_is_then_deleted() {
        let files = device();
        legacy(&files, "aaaaaaaaaaaa", "active", "");
        legacy(&files, "bbbbbbbbbbbb", "active", "");
        let history = Path::new("Tasks/.history/pppppppppppp-2023-11.log");
        let lines = "2023-11-14T22:13:20Z created aaaaaaaaaaaa\n2023-11-15T00:00:00Z moved-to-backlog aaaaaaaaaaaa\n";
        files.load(history, lines.into());
        Tasks::open_in(files.clone(), "Tasks");
        let created = |id| {
            file(&files, &format!("Tasks/{id}.md"))
                .scalar("created")
                .map(str::to_owned)
        };
        assert_eq!(
            created("aaaaaaaaaaaa").as_deref(),
            Some("2023-11-14T22:13:20Z")
        );
        assert!(created("bbbbbbbbbbbb").is_some()); // the modification time
        assert!(!files.exists(history));
    }

    #[test]
    fn a_new_devices_favorites_lose_to_edits_made_elsewhere() {
        let phone = legacy_device();
        let tasks = Tasks::open_in(phone.clone(), "Tasks");
        tasks.edit_filter(ACTIVE.id, "Now", ACTIVE.query).unwrap();
        let path = format!("Filters/{}.md", ACTIVE.id);
        let read = |files: &MemoryFiles| files.read(Path::new(&path)).unwrap();
        // Edited a while before the tablet joins.
        let mut edited = file(&phone, &path);
        edited.set_scalar("updated", "2020-01-01T00:00:00Z");
        phone.write(Path::new(&path), &edited.render()).unwrap();
        let tablet = device();
        Tasks::open_in(tablet.clone(), "Tasks");
        let merged =
            crate::sync::filters::merge(None, read(&tablet).as_bytes(), read(&phone).as_bytes());
        let merged = Document::parse(std::str::from_utf8(&merged.unwrap()).unwrap()).unwrap();
        assert_eq!(merged.scalar("title"), Some("Now"));
        assert_eq!(merged.list("places"), file(&phone, &path).list("places"));
    }

    #[test]
    fn a_filter_that_cannot_be_read_is_left_alone() {
        let files = legacy_device();
        filter(&files, "wwwwwwwwwwww", "@work &", &[]);
        Tasks::open_in(files.clone(), "Tasks");
        let kept = file(&files, "Filters/wwwwwwwwwwww.md");
        assert_eq!(kept.scalar("query"), Some("@work &"));
    }

    #[test]
    fn the_migration_runs_once_per_device() {
        let files = legacy_device();
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        tasks.delete_filter(ACTIVE.id).unwrap();
        let work = tasks.create_filter("Work", "@work").unwrap();
        files.take_changes();
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        assert!(files.take_changes().is_empty());
        let titles: Vec<_> = (tasks.filters().unwrap().into_iter())
            .map(|f| f.title)
            .collect();
        assert_eq!(titles, ["All Tasks", "Backlog", "Work"]);
        assert_eq!(
            file(&files, &format!("Filters/{}.md", work.id)).scalar("query"),
            Some("@work")
        );
    }

    #[test]
    fn devices_migrating_before_they_sync_end_with_one_set_of_the_favorites() {
        let (phone, tablet) = (legacy_device(), legacy_device());
        Tasks::open_in(phone.clone(), "Tasks");
        Tasks::open_in(tablet.clone(), "Tasks");
        let names = |files: &MemoryFiles| {
            let mut names: Vec<_> = (files.list(Path::new("Filters")).unwrap().into_iter())
                .map(|l| l.name)
                .collect();
            names.sort();
            names
        };
        assert_eq!(names(&phone), names(&tablet));
        assert_eq!(names(&phone).len(), 3);
        let path = format!("Filters/{}.md", ACTIVE.id);
        let (a, b) = (
            phone.read(Path::new(&path)).unwrap(),
            tablet.read(Path::new(&path)).unwrap(),
        );
        let merged = crate::sync::filters::merge(None, a.as_bytes(), b.as_bytes()).unwrap();
        let merged = Document::parse(std::str::from_utf8(&merged).unwrap()).unwrap();
        assert_eq!(merged.list("places"), file(&phone, &path).list("places"));
    }

    #[test]
    fn a_device_that_got_migrated_filters_by_sync_does_not_change_them_again() {
        let files = legacy_device();
        filter(
            &files,
            "wwwwwwwwwwww",
            "(@work) & !@backlog",
            &["bbbbbbbbbbbb 1"],
        );
        let before = file(&files, "Filters/wwwwwwwwwwww.md").render();
        Tasks::open_in(files.clone(), "Tasks");
        assert_eq!(file(&files, "Filters/wwwwwwwwwwww.md").render(), before);
    }

    #[test]
    fn a_new_device_starts_with_the_three_favorites_and_no_tasks() {
        let tasks = Tasks::open_in(device(), "Tasks");
        assert_eq!(tasks.filters().unwrap().len(), 3);
        assert!(tasks.rows().unwrap().is_empty());
    }

    #[test]
    fn subtasks_are_numbered_from_their_siblings_old_order() {
        let files = device();
        legacy(&files, "aaaaaaaaaaaa", "backlog", "");
        legacy(
            &files,
            "bbbbbbbbbbbb",
            "active",
            "parent: aaaaaaaaaaaa\nposition: 9\n",
        );
        legacy(
            &files,
            "cccccccccccc",
            "active",
            "parent: aaaaaaaaaaaa\nposition: 3\n",
        );
        legacy(
            &files,
            "dddddddddddd",
            "active",
            "parent: aaaaaaaaaaaa\norder: 1\n",
        );
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        let key = |id: &str| {
            file(&files, &format!("Tasks/{id}.md"))
                .scalar(SUBTASK_POSITION)
                .map(str::to_owned)
        };
        assert_eq!(key("aaaaaaaaaaaa"), None);
        assert_eq!(
            ["cccccccccccc", "bbbbbbbbbbbb", "dddddddddddd"].map(key),
            [Some("1".into()), Some("2".into()), Some("3".into())]
        );
        let sheet = ids(tasks.subtasks("aaaaaaaaaaaa").unwrap());
        assert_eq!(sheet, ["cccccccccccc", "bbbbbbbbbbbb", "dddddddddddd"]);
    }
}
