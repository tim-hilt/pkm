//! Filters (ADR-0024): `Filters/<id>.md` beside the Tasks folder, each a Title
//! and a Query over the Active Tasks.

use super::{Error, Nodes, Row, Tasks, compare_positions, list, rows_of};
use crate::Planning;
use crate::query::Query;
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::Document;
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::io;
use std::path::PathBuf;

/// A saved Query, as the Filters list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub id: String,
    pub title: String,
    /// As the user typed it.
    pub query: String,
    pub favorite: bool,
}

/// A Filter with its place in the Filters list.
struct Stored {
    filter: Filter,
    position: f64,
}

impl Tasks {
    /// The folder's name, beside the Tasks folder in the app's storage and in
    /// the cloud.
    pub const FILTERS_FOLDER: &'static str = "Filters";

    /// Creates a Filter at the end of the Filters list. The Title is trimmed and
    /// kept to one line (as a Task's). Fails with `EmptyFilterTitle` or
    /// `InvalidQuery` before saving anything.
    pub fn create_filter(&self, title: &str, query: &str) -> Result<Filter, Error> {
        let title = super::one_line(title);
        if title.is_empty() {
            return Err(Error::EmptyFilterTitle);
        }
        Query::parse(query).map_err(Error::InvalidQuery)?;
        let position = self
            .stored_filters()?
            .last()
            .map_or(1.0, |s| s.position + 1.0);
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let path = self.filter_path(id.as_str());
            if self.files.exists(&path) {
                continue;
            }
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", &title);
            file.set_scalar("query", query);
            file.set_scalar("favorite", "false");
            file.set_scalar("position", position.to_string());
            file.set_scalar("updated", iso8601(now_ms()));
            self.files.write(&path, &file.render())?;
            return Ok(Filter {
                id: id.as_str().to_owned(),
                title,
                query: query.to_owned(),
                favorite: false,
            });
        }
    }

    /// Every Filter in the Filters list's order.
    pub fn filters(&self) -> io::Result<Vec<Filter>> {
        Ok(self
            .stored_filters()?
            .into_iter()
            .map(|s| s.filter)
            .collect())
    }

    /// The Active Tasks the Filter `id` matches, as `rows` lists Active: in its
    /// order, each matched on its own Tags, Blocked ones and Subtasks included,
    /// never the Backlog. A Query that can't be read matches nothing. Fails
    /// with `NotFound` if there is no such Filter.
    pub fn filter_rows(&self, id: &str) -> io::Result<Vec<Row>> {
        let filter = (self.stored_filters()?.into_iter())
            .find(|s| s.filter.id == id)
            .ok_or(io::ErrorKind::NotFound)?
            .filter;
        let nodes: Nodes = self.shown()?;
        let Ok(query) = Query::parse(&filter.query) else {
            return Ok(Vec::new());
        };
        let found = list(&nodes, Planning::Active)
            .into_iter()
            .filter(|n| query.matches(&n.task.tags))
            .collect();
        Ok(rows_of(&nodes, found))
    }

    fn filters_folder(&self) -> PathBuf {
        let beside = self.root.parent().unwrap_or(std::path::Path::new(""));
        beside.join(Self::FILTERS_FOLDER)
    }

    fn filter_path(&self, id: &str) -> PathBuf {
        self.filters_folder().join(format!("{id}.md"))
    }

    /// The readable Filter files, in the Filters list's order. Only `<id>.md`
    /// is the Filter `id`.
    fn stored_filters(&self) -> io::Result<Vec<Stored>> {
        let mut found: Vec<Stored> = (self.files.list(&self.filters_folder())?.iter())
            .filter_map(|f| {
                let text = self.files.read(&self.filters_folder().join(&f.name)).ok()?;
                let stored = stored_of(&Document::parse(&text)?)?;
                (f.name == format!("{}.md", stored.filter.id)).then_some(stored)
            })
            .collect();
        found.sort_by(|a, b| {
            compare_positions(a.position, b.position).then_with(|| a.filter.id.cmp(&b.filter.id))
        });
        Ok(found)
    }
}

/// The Filter a file describes; `None` for one without an id, Title or Query.
fn stored_of(file: &Document) -> Option<Stored> {
    Some(Stored {
        filter: Filter {
            id: file.scalar("id")?.to_owned(),
            title: file.scalar("title")?.to_owned(),
            query: file.scalar("query")?.to_owned(),
            favorite: file.scalar("favorite") == Some("true"),
        },
        position: file
            .scalar("position")
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Files, MemoryFiles};
    use std::path::Path;
    use std::sync::Arc;

    fn open() -> (Arc<MemoryFiles>, Tasks) {
        let files = Arc::new(MemoryFiles::default());
        let tasks = Tasks::open_in(
            files.clone(),
            "Tasks",
            ItemId::parse("dddddddddddd").unwrap(),
            None,
        );
        (files, tasks)
    }

    fn add(tasks: &Tasks, title: &str, planning: Planning, tags: &[&str]) -> String {
        let tags: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
        tasks.create(title, planning, 4, &tags).unwrap().id
    }

    fn titles(rows: Vec<Row>) -> Vec<String> {
        rows.into_iter().map(|r| r.task.title).collect()
    }

    #[test]
    fn a_new_filter_goes_at_the_end_of_the_list() {
        let (_, tasks) = open();
        assert!(tasks.filters().unwrap().is_empty());
        let work = tasks.create_filter("Work", "@work & !@in-review").unwrap();
        tasks.create_filter("  Home\n", "@home").unwrap();
        let listed = tasks.filters().unwrap();
        assert_eq!(listed[0], work);
        assert_eq!(
            listed.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(),
            ["Work", "Home"]
        );
        assert_eq!(listed[0].query, "@work & !@in-review");
        assert!(!listed[0].favorite);
    }

    #[test]
    fn a_filter_is_a_file_beside_the_tasks() {
        let (files, tasks) = open();
        let filter = tasks.create_filter("Work", "@work | @a").unwrap();
        let text = files
            .read(Path::new(&format!("Filters/{}.md", filter.id)))
            .unwrap();
        let file = Document::parse(&text).unwrap();
        assert_eq!(file.scalar("id"), Some(filter.id.as_str()));
        assert_eq!(file.scalar("title"), Some("Work"));
        assert_eq!(file.scalar("query"), Some("@work | @a"));
        assert_eq!(file.scalar("favorite"), Some("false"));
        assert_eq!(file.scalar("position"), Some("1"));
        assert!(file.scalar("updated").is_some());
    }

    #[test]
    fn an_empty_title_or_an_unparsable_query_is_refused_and_saves_nothing() {
        let (files, tasks) = open();
        assert!(matches!(
            tasks.create_filter(" \n ", "@a"),
            Err(Error::EmptyFilterTitle)
        ));
        let Err(Error::InvalidQuery(e)) = tasks.create_filter("Work", "@a &") else {
            panic!("expected InvalidQuery");
        };
        assert_eq!(e.position, 4);
        assert!(files.list(Path::new("Filters")).unwrap().is_empty());
    }

    #[test]
    fn two_filters_may_share_a_title() {
        let (_, tasks) = open();
        tasks.create_filter("Same", "@a").unwrap();
        tasks.create_filter("Same", "@b").unwrap();
        assert_eq!(tasks.filters().unwrap().len(), 2);
    }

    #[test]
    fn a_filter_shows_the_active_tasks_its_query_matches_in_actives_order() {
        let (_, tasks) = open();
        add(&tasks, "one", Planning::Active, &["work"]);
        add(&tasks, "skipped", Planning::Active, &["home"]);
        add(&tasks, "two", Planning::Active, &["work", "x"]);
        add(
            &tasks,
            "reviewing",
            Planning::Active,
            &["work", "in-review"],
        );
        add(&tasks, "later", Planning::Backlog, &["work"]);
        let filter = tasks.create_filter("Work", "@work & !@in-review").unwrap();
        assert_eq!(
            titles(tasks.filter_rows(&filter.id).unwrap()),
            ["one", "two"]
        );
    }

    #[test]
    fn tasks_are_matched_on_their_own_tags_blocked_and_subtasks_included() {
        let (_, tasks) = open();
        let parent = add(&tasks, "parent", Planning::Active, &["work"]);
        let blocker = add(&tasks, "blocker", Planning::Active, &[]);
        let blocked = add(&tasks, "blocked", Planning::Active, &["work"]);
        tasks.add_blocker(&blocked, &blocker).unwrap();
        tasks
            .create_subtask(&parent, "sub", 4, &["work".to_owned()])
            .unwrap();
        tasks
            .create_subtask(&parent, "untagged sub", 4, &[])
            .unwrap();
        let filter = tasks.create_filter("Work", "@work").unwrap();
        let rows = tasks.filter_rows(&filter.id).unwrap();
        assert_eq!(
            rows.iter()
                .map(|r| r.task.title.as_str())
                .collect::<Vec<_>>(),
            ["parent", "sub", "blocked"]
        );
        assert_eq!(rows[1].parent_title.as_deref(), Some("parent"));
        assert!(rows[2].task.blocked);
    }

    #[test]
    fn a_task_leaves_the_filter_when_it_no_longer_matches() {
        let (_, tasks) = open();
        let id = add(&tasks, "t", Planning::Active, &["work"]);
        let filter = tasks.create_filter("Work", "@work").unwrap();
        assert_eq!(titles(tasks.filter_rows(&filter.id).unwrap()), ["t"]);
        tasks.remove_tag(&id, "work").unwrap();
        assert!(tasks.filter_rows(&filter.id).unwrap().is_empty());
        tasks.add_tag(&id, "work").unwrap();
        tasks.plan(&id, Planning::Backlog).unwrap();
        assert!(tasks.filter_rows(&filter.id).unwrap().is_empty());
    }

    #[test]
    fn an_unreadable_query_matches_nothing_and_an_unknown_filter_is_not_found() {
        let (files, tasks) = open();
        add(&tasks, "t", Planning::Active, &["work"]);
        let filter = tasks.create_filter("Work", "@work").unwrap();
        let path = format!("Filters/{}.md", filter.id);
        let mut file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        file.set_scalar("query", "@work &");
        files.write(Path::new(&path), &file.render()).unwrap();
        assert_eq!(tasks.filters().unwrap().len(), 1);
        assert!(tasks.filter_rows(&filter.id).unwrap().is_empty());
        let missing = tasks.filter_rows("aaaaaaaaaaaa").unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }
}
