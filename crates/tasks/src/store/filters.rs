//! Filters (ADR-0024): `Filters/<id>.md` beside the Tasks folder, each a Title
//! and a Query over the Open Tasks, optionally Grouped by Tags (ADR-0027).

use super::{
    Anchor, Error, Group, Node, Nodes, Tasks, compare_positions, end_key, in_subtask_order,
};
use super::{place, place_among, rows_of};
use crate::query::{Query, QueryError};
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::front_matter::{Document, Value};
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use std::collections::{BTreeSet, HashMap};
use std::io;
use std::iter;
use std::path::PathBuf;

/// What a Filter orders by: the Sort key of a `FilterSort`. A new key is one
/// more variant here and in `FilterSort`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSortKey {
    Manual,
    Priority,
}

impl FilterSortKey {
    /// Every key, in the order a menu lists them.
    pub const ALL: [Self; 2] = [Self::Manual, Self::Priority];
}

/// Which end of a Sort key comes first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// P1 first.
    Ascending,
    /// P4 first.
    Descending,
}

/// How a Filter orders its Tasks (ADR-0025). Stored as one scalar (`sort`), so
/// key and Direction merge as a pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterSort {
    /// By the Filter's places, see `Tasks::move_in_filter`.
    Manual,
    /// By Priority; Tasks of one Priority keep their Manual order.
    Priority(Direction),
}

impl FilterSort {
    pub fn key(self) -> FilterSortKey {
        match self {
            Self::Manual => FilterSortKey::Manual,
            Self::Priority(_) => FilterSortKey::Priority,
        }
    }

    /// This Sort with another key: the same Direction if the key stays
    /// Priority, else the key's default (P1 first).
    #[must_use]
    pub fn with_key(self, key: FilterSortKey) -> Self {
        match (key, self) {
            (FilterSortKey::Manual, _) => Self::Manual,
            (FilterSortKey::Priority, Self::Priority(_)) => self,
            (FilterSortKey::Priority, Self::Manual) => Self::Priority(Direction::Ascending),
        }
    }

    /// The file's value; an absent or unknown one is Manual (a later Sort key
    /// must not break an older Device).
    fn read(value: Option<&str>) -> Self {
        match value {
            Some("priority ascending") => Self::Priority(Direction::Ascending),
            Some("priority descending") => Self::Priority(Direction::Descending),
            _ => Self::Manual,
        }
    }

    fn write(self, file: &mut Document) {
        match self {
            Self::Manual => file.remove("sort"),
            Self::Priority(Direction::Ascending) => file.set_scalar("sort", "priority ascending"),
            Self::Priority(Direction::Descending) => file.set_scalar("sort", "priority descending"),
        }
    }

    /// The position of a Task of `priority` among the others: lower is first.
    fn rank(self, priority: u8) -> i16 {
        match self {
            Self::Manual => 0,
            Self::Priority(Direction::Ascending) => i16::from(priority),
            Self::Priority(Direction::Descending) => -i16::from(priority),
        }
    }
}

/// A saved Query, as the Filters list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub id: String,
    pub title: String,
    /// As the user typed it.
    pub query: String,
    pub favorite: bool,
    pub sort: FilterSort,
    /// The Tags the Filter's Tasks are split by, in the order of their Groups,
    /// without `@`; none if it isn't Grouped.
    pub group_by: Vec<String>,
    /// How many Open Tasks the Query matches.
    pub matches: u32,
    /// Why the stored Query can't be read (a hand edit, a newer Device); such a
    /// Filter matches nothing until it is edited.
    pub error: Option<String>,
}

/// A Filter's own order (ADR-0027): per Group, by its Tag (none for the Tasks
/// without a Group), a place per Task ID, see `in_filter_order`.
pub(super) type Places = HashMap<Option<String>, Vec<(String, f64)>>;

/// A Filter with its place in the Filters list.
pub(super) struct Stored {
    pub(super) filter: Filter,
    pub(super) position: f64,
    pub(super) places: Places,
}

impl Tasks {
    /// The folder's name, beside the Tasks folder in the app's storage and in
    /// the cloud.
    pub const FILTERS_FOLDER: &'static str = "Filters";

    /// Creates a Filter at the end of the Filters list. The Title is trimmed and
    /// kept to one line (as a Task's). `group_by` is typed as `@work @private`,
    /// see `check_group_by`. Fails with `EmptyFilterTitle`, `InvalidQuery` or
    /// a Group-by error before saving anything.
    pub fn create_filter(&self, title: &str, query: &str, group_by: &str) -> Result<Filter, Error> {
        let title = checked(title, query)?;
        let group_by = self.check_group_by(group_by)?;
        let positions: Vec<f64> = self.stored_filters()?.iter().map(|s| s.position).collect();
        let position = end_key(&positions);
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            let path = self.filter_path(&id);
            if self.files.exists(&path) {
                continue;
            }
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar("title", &title);
            file.set_scalar("query", query);
            write_group_by(&mut file, &group_by);
            file.set_scalar("favorite", "false");
            file.set_scalar("position", position.to_string());
            file.set_scalar("updated", iso8601(now_ms()));
            self.files.write(&path, &file.render())?;
            return Ok(self.filter(id.as_str())?);
        }
    }

    /// Changes the Title, Query and Group by of the Filter `id`, which keeps its
    /// place, Favorite flag and order. A Tag newly in Group by starts its Group
    /// in the order the Filter showed before; one taken out loses its Group's
    /// order. Refuses what `create_filter` refuses,
    /// before saving anything, and fails with `NotFound` if there is no such
    /// Filter.
    pub fn edit_filter(
        &self,
        id: &str,
        title: &str,
        query: &str,
        group_by: &str,
    ) -> Result<Filter, Error> {
        let title = checked(title, query)?;
        let group_by = self.check_group_by(group_by)?;
        let seeds = self.seeded_places(id, query, &group_by)?;
        self.rewrite_filter(id, |file| {
            file.set_scalar("title", &title);
            file.set_scalar("query", query);
            write_group_by(file, &group_by);
            let mut places = places_of(file);
            places.extend(seeds);
            set_places(file, &places);
        })?;
        Ok(self.filter(id)?)
    }

    /// Marks the Filter `id` as a Favorite, or unmarks it. Fails with `NotFound`
    /// if there is no such Filter.
    pub fn set_favorite(&self, id: &str, favorite: bool) -> io::Result<()> {
        self.rewrite_filter(id, |file| file.set_scalar("favorite", favorite.to_string()))
    }

    /// Moves the Filter `id` before the Filter `before` in the Filters list, or
    /// to its end if that is `None`; a drop where it already is changes
    /// nothing. Usually only its file is written, as for Tasks (see `place`).
    /// Fails with `NotFound` for an unknown Filter or a `before` that isn't in
    /// the list.
    pub fn move_filter(&self, id: &str, before: Option<&str>) -> io::Result<()> {
        let mut order = self.stored_filters()?;
        let from = (order.iter().position(|s| s.filter.id == id)).ok_or(io::ErrorKind::NotFound)?;
        if before == Some(id) {
            return Ok(());
        }
        order.remove(from);
        let to = match before {
            None => order.len(),
            Some(b) => {
                (order.iter().position(|s| s.filter.id == b)).ok_or(io::ErrorKind::NotFound)?
            }
        };
        if to == from {
            return Ok(());
        }
        let positions: Vec<f64> = order.iter().map(|s| s.position).collect();
        let placement = place(&positions, to);
        for (at, position) in placement.shifted {
            self.set_position(&order[at].filter.id, position)?;
        }
        self.set_position(id, placement.position)
    }

    /// Deletes the Filter `id`. No Task changes. Fails with `NotFound` if there
    /// is no such Filter.
    pub fn delete_filter(&self, id: &str) -> io::Result<()> {
        self.files.remove(&self.filter_path(&filter_id(id)?))
    }

    /// Every Filter in the Filters list's order, with how many Open Tasks it
    /// matches.
    pub fn filters(&self) -> io::Result<Vec<Filter>> {
        let stored = self.stored_filters()?;
        let nodes: Nodes = self.shown()?;
        let open: Vec<&Node> = nodes.values().collect();
        Ok(stored
            .into_iter()
            .map(|s| described(s.filter, &open))
            .collect())
    }

    fn filter(&self, id: &str) -> io::Result<Filter> {
        (self.filters()?.into_iter())
            .find(|f| f.id == id)
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    /// The Groups of the Filter `id`, in display order: first the one without
    /// a Tag (even if empty) holding the Open Tasks carrying none of the
    /// Filter's Group-by Tags, then one per Group-by Tag (even if empty) with
    /// the Tasks carrying it, so a Task with several is in several Groups.
    /// Open Tasks are those the Filter matches: each matched on its own Tags,
    /// Blocked ones, Subtasks and Backlog Tasks included. In every Group the
    /// Tasks with a place come first in the Filter's order, then the others by
    /// `created`; if the Filter is Sorted, then by that, Tasks of one key
    /// keeping that order; the Subtasks of one Parent keep their Subtask
    /// order among themselves. A Query that can't be read matches nothing.
    /// Fails with `NotFound` if there is no such Filter.
    pub fn filter_groups(&self, id: &str) -> io::Result<Vec<Group>> {
        let filter = self.stored_filter(id)?;
        let nodes: Nodes = self.shown()?;
        Ok(shown_groups(&nodes, &filter)
            .into_iter()
            .map(|(tag, found)| Group {
                tag,
                rows: rows_of(&nodes, found),
            })
            .collect())
    }

    /// The Tags a Task needs to stay in the Filter `id` (see
    /// `Query::required_tags`), which a new Task starts with; none if there is
    /// no such Filter or its Query can't be read.
    pub fn required_tags(&self, id: &str) -> io::Result<Vec<String>> {
        let stored = self
            .stored_filters()?
            .into_iter()
            .find(|s| s.filter.id == id);
        Ok((stored.and_then(|s| Query::parse(&s.filter.query).ok()))
            .map(|q| q.required_tags())
            .unwrap_or_default())
    }

    /// Moves the Task `task` before the Task `before` in the Group `group` of
    /// the Filter `id` (none: among the Tasks without a Group), or to the end
    /// of it if `before` is `None`; a drop where it already is changes nothing.
    /// Only that Group's order changes. Usually only the moved Task gets a
    /// place in the Filter's file, as for Tasks (see `place`). If another Task
    /// shown in the Group has none yet, or the Subtask order refilled the
    /// rows, every Task shown in it gets one, in one write.
    ///
    /// In a Sorted Filter a drop between Tasks of the moved Task's own key
    /// keeps the Sort and places it among those. Any other drop gives every
    /// Task shown in every Group a place in the order shown with the drop
    /// applied, and turns the Filter Manual, in the same write.
    ///
    /// A Subtask dropped past a sibling shown also moves in its Subtask order,
    /// to just after the last sibling it now follows, or before the first
    /// it now precedes; that writes its Task file (and those sharing its key).
    /// Fails with `NotFound` for an unknown Filter, a `group` the Filter
    /// doesn't have, or a `task` or `before` the Group doesn't show.
    pub fn move_in_filter(
        &self,
        id: &str,
        group: Option<&str>,
        task: &str,
        before: Option<&str>,
    ) -> io::Result<()> {
        let mut filter = self.stored_filter(id)?;
        let sort = filter.filter.sort;
        let nodes: Nodes = self.shown()?;
        let mut groups = shown_groups(&nodes, &filter);
        let group_at = (groups.iter().position(|(tag, _)| tag.as_deref() == group))
            .ok_or(io::ErrorKind::NotFound)?;
        let (group_tag, mut shown) = groups.remove(group_at);
        let from = (shown.iter().position(|n| n.task.id == task)).ok_or(io::ErrorKind::NotFound)?;
        if before == Some(task) {
            return Ok(());
        }
        let moved = shown.remove(from);
        let to = match before {
            None => shown.len(),
            Some(b) => {
                (shown.iter().position(|n| n.task.id == b)).ok_or(io::ErrorKind::NotFound)?
            }
        };
        if to == from {
            return Ok(());
        }
        let rank = |n: &Node| sort.rank(n.task.priority);
        let own = rank(moved);
        let in_own_key = sort != FilterSort::Manual
            && shown[..to].last().is_none_or(|n| rank(n) <= own)
            && shown.get(to).is_none_or(|n| rank(n) >= own);
        let own_places = filter.places.remove(&group_tag).unwrap_or_default();
        let moved_places = if in_own_key {
            let (own_key, at) = among_own_key(&shown, to, own, rank);
            placed_at(&own_places, &own_key, at, task)
        } else {
            placed_at(&own_places, &shown, to, task)
        };
        let turns_manual = sort != FilterSort::Manual && !in_own_key;
        let mut places = filter.places;
        if turns_manual {
            places.extend(
                groups
                    .iter()
                    .map(|(tag, rows)| (tag.clone(), numbered(rows))),
            );
        }
        places.insert(group_tag, moved_places);
        self.rewrite_filter(id, |file| {
            set_places(file, &places);
            if turns_manual {
                FilterSort::Manual.write(file);
            }
        })?;

        let Some(parent) = moved.task.parent.as_deref() else {
            return Ok(());
        };
        let is_sibling = |n: &&&Node| n.task.parent.as_deref() == Some(parent);
        let passed = shown[..from].iter().filter(is_sibling).count()
            != shown[..to].iter().filter(is_sibling).count();
        let follows = shown[..to].iter().rfind(is_sibling);
        let precedes = shown[to..].iter().find(is_sibling);
        let anchor = match (follows, precedes) {
            _ if !passed => return Ok(()),
            (Some(n), _) => Anchor::After(&n.task.id),
            (None, Some(n)) => Anchor::Before(&n.task.id),
            (None, None) => return Ok(()),
        };
        self.place_subtask(&nodes, moved, parent, anchor)
            .map_err(Error::into_io)
    }

    /// Sorts the Filter `id`. Manual keeps the order shown: every Task shown
    /// in every Group gets a place for it. Priority from Manual discards the places; a new
    /// Direction of Priority keeps them, so the Tasks of one Priority stay in
    /// their order. Writes no Task file. Fails with `NotFound` if there is no
    /// such Filter.
    pub fn set_sort(&self, id: &str, sort: FilterSort) -> io::Result<()> {
        let filter = self.stored_filter(id)?;
        let was = filter.filter.sort;
        let places = match (sort, was) {
            (FilterSort::Manual, FilterSort::Manual)
            | (FilterSort::Priority(_), FilterSort::Priority(_)) => filter.places,
            (FilterSort::Priority(_), FilterSort::Manual) => Places::new(),
            (FilterSort::Manual, FilterSort::Priority(_)) => {
                let nodes: Nodes = self.shown()?;
                (shown_groups(&nodes, &filter).into_iter())
                    .map(|(tag, rows)| (tag, numbered(&rows)))
                    .collect()
            }
        };
        self.rewrite_filter(id, |file| {
            set_places(file, &places);
            sort.write(file);
        })
    }

    /// Checks a Query text without saving: the Tags it names, Wildcards
    /// included, that fit no known Tag (marked as such, and no reason to
    /// refuse it), or why it can't be read.
    pub fn check_query(&self, text: &str) -> Result<Vec<String>, QueryError> {
        let tags = self.read_tags();
        let known: Vec<&str> = tags.names().collect();
        Ok(Query::parse(text)?.unmatched(&known))
    }

    /// Checks a Group-by text without saving: the Tags it lists, in order and
    /// without repeats, or why it can't be read. Each Tag is typed with or
    /// without `@`, separated by whitespace; empty text lists none. Fails with
    /// `UnknownGroupByTag` for a Tag that doesn't exist, and
    /// `InvalidGroupByTag` for a Wildcard or anything else that isn't a Tag.
    pub fn check_group_by(&self, text: &str) -> Result<Vec<String>, Error> {
        let known = self.read_tags();
        let mut tags: Vec<String> = Vec::new();
        for word in text.split_whitespace() {
            let tag = word.strip_prefix('@').unwrap_or(word);
            if !pkm_base::tag::is_valid(tag) {
                return Err(Error::InvalidGroupByTag(word.to_owned()));
            }
            if !known.contains(tag) {
                return Err(Error::UnknownGroupByTag(tag.to_owned()));
            }
            if !tags.iter().any(|t| t == tag) {
                tags.push(tag.to_owned());
            }
        }
        Ok(tags)
    }

    /// The places of the Groups `group_by` adds to the Filter `id`, so that each
    /// starts in the order the Filter showed before (a Task of several Groups
    /// at its first place); Tasks it didn't show follow in the order created.
    fn seeded_places(&self, id: &str, query: &str, group_by: &[String]) -> io::Result<Places> {
        let filter = self.stored_filter(id)?;
        let added: Vec<&String> = (group_by.iter())
            .filter(|tag| !filter.filter.group_by.contains(tag))
            .collect();
        if added.is_empty() {
            return Ok(Places::new());
        }
        let nodes: Nodes = self.shown()?;
        let mut shown_at: HashMap<&str, usize> = HashMap::new();
        for (_, rows) in shown_groups(&nodes, &filter) {
            for n in rows {
                let next = shown_at.len();
                shown_at.entry(n.task.id.as_str()).or_insert(next);
            }
        }
        let mut found = matching(by_creation(&nodes), query);
        found.sort_by_key(|n| {
            shown_at
                .get(n.task.id.as_str())
                .copied()
                .unwrap_or(usize::MAX)
        });
        Ok(added
            .into_iter()
            .map(|tag| {
                let members: Vec<&Node> = (found.iter().copied())
                    .filter(|n| is_in_group(n, Some(tag), group_by))
                    .collect();
                (Some(tag.clone()), numbered(&members))
            })
            .collect())
    }

    /// Takes `tag` out of the Group by of every Filter that has it, with its
    /// places. Safe to repeat.
    pub(super) fn remove_from_group_bys(&self, tag: &str) -> io::Result<()> {
        for s in self.stored_filters()? {
            if s.filter.group_by.iter().any(|t| t == tag) {
                self.rewrite_filter(&s.filter.id, |file| {
                    let mut group_by = group_by_of(file);
                    group_by.retain(|t| t != tag);
                    write_group_by(file, &group_by);
                })?;
            }
        }
        Ok(())
    }

    fn filters_folder(&self) -> PathBuf {
        let beside = self.root.parent().unwrap_or(std::path::Path::new(""));
        beside.join(Self::FILTERS_FOLDER)
    }

    pub(super) fn filter_path(&self, id: &ItemId) -> PathBuf {
        self.filters_folder().join(format!("{id}.md"))
    }

    fn stored_filter(&self, id: &str) -> io::Result<Stored> {
        (self.stored_filters()?.into_iter())
            .find(|s| s.filter.id == id)
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn set_position(&self, id: &str, position: f64) -> io::Result<()> {
        self.rewrite_filter(id, |file| file.set_scalar("position", position.to_string()))
    }

    /// Applies `change` to the file of the Filter `id`, drops the places of
    /// Tasks it no longer shows in their Group and of Groups no longer in its
    /// Group by (ADR-0024, ADR-0027) and stamps it `updated`; a change
    /// that leaves the file as it was writes nothing, as for Tasks. Fails with
    /// `NotFound` if there is no such Filter.
    fn rewrite_filter(&self, id: &str, change: impl FnOnce(&mut Document)) -> io::Result<()> {
        let path = self.filter_path(&filter_id(id)?);
        let mut file =
            Document::parse(&self.files.read(&path)?).ok_or(io::ErrorKind::InvalidData)?;
        let before = file.render();
        change(&mut file);
        // Not `shown`: a Task pending Rejected keeps its place, to be Reverted.
        let nodes = self.load()?;
        let query = file.scalar("query").unwrap_or_default();
        let shown: HashMap<&str, &Node> = (matching(nodes.values().collect(), query).into_iter())
            .map(|n| (n.task.id.as_str(), n))
            .collect();
        let group_by = group_by_of(&file);
        let mut places = places_of(&file);
        places.retain(|group, _| group.as_ref().is_none_or(|tag| group_by.contains(tag)));
        for (group, list) in &mut places {
            list.retain(|(task, _)| {
                (shown.get(task.as_str()))
                    .is_some_and(|n| is_in_group(n, group.as_deref(), &group_by))
            });
        }
        places.retain(|_, list| !list.is_empty());
        set_places(&mut file, &places);
        if file.render() == before {
            return Ok(());
        }
        file.set_scalar("updated", iso8601(now_ms()));
        self.files.write(&path, &file.render())
    }

    /// The readable Filter files, in the Filters list's order. Only `<id>.md`
    /// is the Filter `id`. A Group-by Tag missing from the Tags file (a Delete
    /// racing an edit) is added back, grey, as a Task's is (ADR-0023).
    pub(super) fn stored_filters(&self) -> io::Result<Vec<Stored>> {
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
        let tags = found.iter().flat_map(|s| s.filter.group_by.iter().cloned());
        self.remember_tags(tags.collect::<BTreeSet<_>>())?;
        Ok(found)
    }
}

/// `id` as a Filter's ID; `NotFound` for one no Filter can have.
fn filter_id(id: &str) -> io::Result<ItemId> {
    ItemId::parse(id).ok_or_else(|| io::ErrorKind::NotFound.into())
}

/// The Open Tasks `query` matches, in the order of `found`; none if it can't
/// be read.
pub(super) fn matching<'a>(found: Vec<&'a Node>, query: &str) -> Vec<&'a Node> {
    let Ok(query) = Query::parse(query) else {
        return Vec::new();
    };
    (found.into_iter())
        .filter(|n| query.matches(&n.task.tags))
        .collect()
}

/// The Open Tasks in the order created, then by Item ID: the base order of
/// every list a Filter shows.
fn by_creation(nodes: &Nodes) -> Vec<&Node> {
    let mut open: Vec<&Node> = nodes.values().collect();
    open.sort_by(|a, b| (a.created.cmp(&b.created)).then_with(|| a.task.id.cmp(&b.task.id)));
    open
}

/// The Open Tasks `query` matches in the Filter's order, see `ordered`.
pub(super) fn shown_in<'a>(
    nodes: &'a Nodes,
    query: &str,
    places: &[(String, f64)],
    sort: FilterSort,
) -> Vec<&'a Node> {
    ordered(matching(by_creation(nodes), query), places, sort)
}

/// `found` (in the order created) in the Filter's order: by `places`, then by
/// `created`, then by Item ID (see `in_filter_order`); then by `sort`, Tasks of
/// one key keeping that order; then the rows of each Parent's Subtasks are
/// refilled in their Subtask order (`in_subtask_order`).
fn ordered<'a>(found: Vec<&'a Node>, places: &[(String, f64)], sort: FilterSort) -> Vec<&'a Node> {
    let mut found = in_filter_order(found, places);
    found.sort_by_key(|n| sort.rank(n.task.priority)); // stable
    in_subtask_order(found)
}

/// Whether `node` belongs to the Group `group` of a Filter Grouped by
/// `group_by`: it carries the Tag, or (none) carries none of them.
fn is_in_group(node: &Node, group: Option<&str>, group_by: &[String]) -> bool {
    let carries = |tag: &str| node.task.tags.iter().any(|t| t == tag);
    match group {
        Some(tag) => carries(tag),
        None => !group_by.iter().any(|tag| carries(tag)),
    }
}

/// The Groups of a Filter in display order, each with the Tasks it shows in
/// that Group's own order: first the Group without a Tag, then one per
/// Group-by Tag.
fn shown_groups<'a>(nodes: &'a Nodes, filter: &Stored) -> Vec<(Option<String>, Vec<&'a Node>)> {
    let Filter {
        query,
        sort,
        group_by,
        ..
    } = &filter.filter;
    let found = matching(by_creation(nodes), query);
    let tags = group_by.iter().cloned().map(Some);
    (iter::once(None).chain(tags))
        .map(|group| {
            let members = (found.iter().copied())
                .filter(|n| is_in_group(n, group.as_deref(), group_by))
                .collect();
            let places = filter.places.get(&group).map_or(&[][..], Vec::as_slice);
            (group, ordered(members, places, *sort))
        })
        .collect()
}

/// Places 1, 2, 3 ... for `list`, in its order.
fn numbered(list: &[&Node]) -> Vec<(String, f64)> {
    (list.iter().zip(1..))
        .map(|(n, place)| (n.task.id.clone(), f64::from(place)))
        .collect()
}

/// The Tasks of `shown` with the key of rank `own`, and where a Task of that
/// key dropped at `to` among all of `shown` falls among them.
fn among_own_key<'a>(
    shown: &[&'a Node],
    to: usize,
    own: i16,
    rank: impl Fn(&Node) -> i16,
) -> (Vec<&'a Node>, usize) {
    let at = shown[..to].iter().filter(|n| rank(n) == own).count();
    let group = shown.iter().copied().filter(|n| rank(n) == own);
    (group.collect(), at)
}

/// `places` with the Task `task` put at index `at` of `list` (without it):
/// usually only it changes, see `place_among`.
fn placed_at(
    places: &[(String, f64)],
    list: &[&Node],
    at: usize,
    task: &str,
) -> Vec<(String, f64)> {
    let mut places: HashMap<&str, f64> = places.iter().map(|(id, p)| (id.as_str(), *p)).collect();
    let keys: Vec<Option<f64>> = (list.iter())
        .map(|n| places.get(n.task.id.as_str()).copied())
        .collect();
    let placement = place_among(&keys, at);
    for (at, key) in placement.shifted {
        places.insert(list[at].task.id.as_str(), key);
    }
    places.insert(task, placement.position);
    places
        .into_iter()
        .map(|(id, p)| (id.to_owned(), p))
        .collect()
}

/// `found` as a Filter shows it: the Tasks with a place first, by place, then
/// the rest; equal places and the rest keep the order of `found`.
pub(super) fn in_filter_order<'a>(found: Vec<&'a Node>, places: &[(String, f64)]) -> Vec<&'a Node> {
    let places: HashMap<&str, f64> = places.iter().map(|(id, p)| (id.as_str(), *p)).collect();
    let (mut placed, unplaced): (Vec<&Node>, Vec<&Node>) =
        (found.into_iter()).partition(|n| places.contains_key(n.task.id.as_str()));
    placed
        .sort_by(|a, b| compare_positions(places[a.task.id.as_str()], places[b.task.id.as_str()]));
    placed.extend(unplaced);
    placed
}

/// The `places` entries, `<task-id> <position>` for the Tasks without a Group
/// and `<tag> <task-id> <position>` for a Group; unreadable ones are skipped.
fn places_of(file: &Document) -> Places {
    let mut places = Places::new();
    for entry in file.list("places").unwrap_or_default() {
        let fields: Vec<&str> = entry.split_whitespace().collect();
        let (group, task, position) = match fields[..] {
            [task, position] => (None, task, position),
            [tag, task, position] => (Some(tag.to_owned()), task, position),
            _ => continue,
        };
        if let Ok(position) = position.parse() {
            places
                .entry(group)
                .or_default()
                .push((task.to_owned(), position));
        }
    }
    places
}

/// Writes `places`, sorted so equal orders give equal files; none, no key.
pub(super) fn set_places(file: &mut Document, places: &Places) {
    let mut entries: Vec<String> = (places.iter())
        .flat_map(|(group, list)| {
            let prefix = group
                .as_ref()
                .map_or(String::new(), |tag| format!("{tag} "));
            list.iter().map(move |(id, p)| format!("{prefix}{id} {p}"))
        })
        .collect();
    entries.sort();
    if entries.is_empty() {
        file.remove("places");
    } else {
        file.set("places", Value::List(entries));
    }
}

/// The Tags a Filter file Groups by, in order.
fn group_by_of(file: &Document) -> Vec<String> {
    let text = file.scalar("group-by").unwrap_or_default();
    text.split_whitespace().map(str::to_owned).collect()
}

/// Writes `group_by` as one scalar of Tags separated by spaces; none, no key.
fn write_group_by(file: &mut Document, group_by: &[String]) {
    if group_by.is_empty() {
        file.remove("group-by");
    } else {
        file.set_scalar("group-by", group_by.join(" "));
    }
}

/// The Title, one line and not empty, and the Query, which must parse.
fn checked(title: &str, query: &str) -> Result<String, Error> {
    let title = super::one_line(title);
    if title.is_empty() {
        return Err(Error::EmptyFilterTitle);
    }
    Query::parse(query).map_err(Error::InvalidQuery)?;
    Ok(title)
}

/// `filter` with its match count and Query error, over the `open` Tasks.
fn described(mut filter: Filter, open: &[&Node]) -> Filter {
    match Query::parse(&filter.query) {
        Ok(query) => {
            let matching = open.iter().filter(|n| query.matches(&n.task.tags));
            filter.matches = u32::try_from(matching.count()).unwrap_or(u32::MAX);
        }
        Err(e) => filter.error = Some(e.to_string()),
    }
    filter
}

/// Whether `file` is a Filter: it has an id, Title and Query.
pub(crate) fn is_filter(file: &Document) -> bool {
    stored_of(file).is_some()
}

/// The Filter a file describes; `None` for one without an id, Title or Query.
fn stored_of(file: &Document) -> Option<Stored> {
    Some(Stored {
        filter: Filter {
            id: file.scalar("id")?.to_owned(),
            title: file.scalar("title")?.to_owned(),
            query: file.scalar("query")?.to_owned(),
            favorite: file.scalar("favorite") == Some("true"),
            sort: FilterSort::read(file.scalar("sort")),
            group_by: group_by_of(file),
            matches: 0,
            error: None,
        },
        position: file
            .scalar("position")
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0),
        places: places_of(file),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Files, MemoryFiles, Row};
    use std::path::Path;
    use std::sync::Arc;

    fn open() -> (Arc<MemoryFiles>, Tasks) {
        let files = Arc::new(MemoryFiles::default());
        let tasks = Tasks::open_in(files.clone(), "Tasks");
        (files, tasks)
    }

    fn add(tasks: &Tasks, title: &str, tags: &[&str]) -> String {
        let tags: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
        tasks.create(title, 4, &tags).unwrap().id
    }

    fn titles(rows: Vec<Row>) -> Vec<String> {
        rows.into_iter().map(|r| r.task.title).collect()
    }

    /// The rows of the Filter's only Group, which has no Tag.
    fn rows_of_filter(tasks: &Tasks, id: &str) -> Vec<Row> {
        let mut groups = tasks.filter_groups(id).unwrap();
        assert_eq!(groups.len(), 1);
        let group = groups.remove(0);
        assert_eq!(group.tag, None);
        group.rows
    }

    #[test]
    fn a_new_filter_goes_at_the_end_of_the_list() {
        let (_, tasks) = open();
        assert!(tasks.filters().unwrap().is_empty());
        let work = tasks
            .create_filter("Work", "@work & !@in-review", "")
            .unwrap();
        tasks.create_filter("  Home\n", "@home", "").unwrap();
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
        let filter = tasks.create_filter("Work", "@work | @a", "").unwrap();
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
            tasks.create_filter(" \n ", "@a", ""),
            Err(Error::EmptyFilterTitle)
        ));
        let Err(Error::InvalidQuery(e)) = tasks.create_filter("Work", "@a &", "") else {
            panic!("expected InvalidQuery");
        };
        assert_eq!(e.position, 4);
        assert!(files.list(Path::new("Filters")).unwrap().is_empty());
    }

    #[test]
    fn checking_a_query_names_its_unknown_tags_or_its_error_and_saves_nothing() {
        let (files, tasks) = open();
        add(&tasks, "t", &["work"]);
        assert_eq!(
            tasks.check_query("@work & !@nope | @x"),
            Ok(vec!["nope".into(), "x".into()])
        );
        assert_eq!(tasks.check_query("@work"), Ok(vec![]));
        assert_eq!(tasks.check_query("@wo* | @*nope"), Ok(vec!["*nope".into()]));
        assert_eq!(tasks.check_query("@work &").unwrap_err().position, 7);
        assert!(files.list(Path::new("Filters")).unwrap().is_empty());
    }

    #[test]
    fn deleting_a_tag_leaves_every_filters_query_as_it_is() {
        let (_, tasks) = open();
        add(&tasks, "t", &["x"]);
        tasks.create_filter("F", "!@x & @y", "").unwrap();
        tasks.remove_tag_everywhere("x").unwrap();
        tasks.forget_tag("x").unwrap();
        let filter = &tasks.filters().unwrap()[0];
        assert_eq!(filter.query, "!@x & @y");
        assert_eq!(
            tasks.check_query(&filter.query),
            Ok(vec!["x".into(), "y".into()])
        );
    }

    #[test]
    fn a_filter_is_marked_and_unmarked_as_a_favorite_keeping_its_place() {
        let (files, tasks) = open();
        let a = tasks.create_filter("A", "@a", "").unwrap();
        tasks.create_filter("B", "@b", "").unwrap();
        tasks.set_favorite(&a.id, true).unwrap();
        let listed = tasks.filters().unwrap();
        assert_eq!(
            listed.iter().map(|f| f.favorite).collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(names(&tasks), ["A", "B"]);
        let text = files
            .read(Path::new(&format!("Filters/{}.md", a.id)))
            .unwrap();
        assert_eq!(
            Document::parse(&text).unwrap().scalar("favorite"),
            Some("true")
        );
        tasks.set_favorite(&a.id, false).unwrap();
        assert!(!tasks.filters().unwrap()[0].favorite);
        let missing = tasks.set_favorite("aaaaaaaaaaaa", true).unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn two_filters_may_share_a_title() {
        let (_, tasks) = open();
        tasks.create_filter("Same", "@a", "").unwrap();
        tasks.create_filter("Same", "@b", "").unwrap();
        assert_eq!(tasks.filters().unwrap().len(), 2);
    }

    #[test]
    fn a_filter_shows_the_open_tasks_its_query_matches_the_backlog_included() {
        let (_, tasks) = open();
        add(&tasks, "one", &["work"]);
        add(&tasks, "skipped", &["home"]);
        add(&tasks, "two", &["work", "x"]);
        add(&tasks, "reviewing", &["work", "in-review"]);
        add(&tasks, "later", &["work", "backlog"]);
        let filter = tasks
            .create_filter("Work", "@work & !@in-review", "")
            .unwrap();
        assert_eq!(
            titles(rows_of_filter(&tasks, &filter.id)),
            ["one", "two", "later"]
        );
    }

    #[test]
    fn tasks_are_matched_on_their_own_tags_blocked_and_subtasks_included() {
        let (_, tasks) = open();
        let parent = add(&tasks, "parent", &["work"]);
        let blocker = add(&tasks, "blocker", &[]);
        let blocked = add(&tasks, "blocked", &["work"]);
        tasks.add_blocker(&blocked, &blocker).unwrap();
        tasks
            .create_subtask(&parent, "sub", 4, &["work".to_owned()])
            .unwrap();
        let untagged = tasks
            .create_subtask(&parent, "untagged sub", 4, &[])
            .unwrap();
        tasks.remove_tag(&untagged.id, "work").unwrap();
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        let rows = rows_of_filter(&tasks, &filter.id);
        assert_eq!(
            rows.iter()
                .map(|r| r.task.title.as_str())
                .collect::<Vec<_>>(),
            ["parent", "blocked", "sub"]
        );
        assert_eq!(rows[2].parent_title.as_deref(), Some("parent"));
        assert!(rows[1].task.blocked);
    }

    #[test]
    fn a_filter_requires_the_tags_of_its_and_chain() {
        let (_, tasks) = open();
        let tags_in = |id: &str| tasks.required_tags(id).unwrap();
        let work = tasks
            .create_filter("F", "@work & !@review & @a", "")
            .unwrap();
        assert_eq!(tags_in(&work.id), ["work", "a"]);
        for query in ["@a | @b", "@*x", "*"] {
            let filter = tasks.create_filter("F", query, "").unwrap();
            assert!(tags_in(&filter.id).is_empty());
        }
        assert!(tags_in("aaaaaaaaaaaa").is_empty()); // gone
    }

    #[test]
    fn a_star_filter_shows_tasks_with_and_without_tags() {
        let (_, tasks) = open();
        add(&tasks, "Plain", &[]);
        add(&tasks, "Tagged", &["a"]);
        let filter = tasks.create_filter("All", "*", "").unwrap();
        assert_eq!(
            titles(rows_of_filter(&tasks, &filter.id)),
            ["Plain", "Tagged"]
        );
    }

    #[test]
    fn a_task_leaves_the_filter_when_it_no_longer_matches() {
        let (_, tasks) = open();
        let id = add(&tasks, "t", &["work"]);
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        assert_eq!(titles(rows_of_filter(&tasks, &filter.id)), ["t"]);
        tasks.remove_tag(&id, "work").unwrap();
        assert!(rows_of_filter(&tasks, &filter.id).is_empty());
        tasks.add_tag(&id, "work").unwrap();
        tasks.add_tag(&id, "backlog").unwrap();
        assert_eq!(titles(rows_of_filter(&tasks, &filter.id)), ["t"]);
        tasks.add_tag(&id, "x").unwrap();
        tasks.remove_tag(&id, "work").unwrap();
        assert!(rows_of_filter(&tasks, &filter.id).is_empty());
    }

    #[test]
    fn an_unreadable_query_matches_nothing_and_an_unknown_filter_is_not_found() {
        let (files, tasks) = open();
        add(&tasks, "t", &["work"]);
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        let path = format!("Filters/{}.md", filter.id);
        let mut file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        file.set_scalar("query", "@work &");
        files.write(Path::new(&path), &file.render()).unwrap();
        assert_eq!(tasks.filters().unwrap().len(), 1);
        assert!(rows_of_filter(&tasks, &filter.id).is_empty());
        let missing = tasks.filter_groups("aaaaaaaaaaaa").unwrap_err();
        assert_eq!(missing.kind(), io::ErrorKind::NotFound);
    }

    fn names(tasks: &Tasks) -> Vec<String> {
        tasks
            .filters()
            .unwrap()
            .into_iter()
            .map(|f| f.title)
            .collect()
    }

    #[test]
    fn editing_changes_title_and_query_and_keeps_the_place() {
        let (_, tasks) = open();
        let a = tasks.create_filter("A", "@a", "").unwrap();
        tasks.create_filter("B", "@b", "").unwrap();
        let edited = tasks.edit_filter(&a.id, " New\n", "@x | @y", "").unwrap();
        assert_eq!(
            (edited.title.as_str(), edited.query.as_str()),
            ("New", "@x | @y")
        );
        assert_eq!(names(&tasks), ["New", "B"]);
    }

    #[test]
    fn editing_refuses_what_creating_refuses_and_saves_nothing() {
        let (_, tasks) = open();
        let a = tasks.create_filter("A", "@a", "").unwrap();
        assert!(matches!(
            tasks.edit_filter(&a.id, " ", "@a", ""),
            Err(Error::EmptyFilterTitle)
        ));
        assert!(matches!(
            tasks.edit_filter(&a.id, "A", "@a |", ""),
            Err(Error::InvalidQuery(_))
        ));
        assert_eq!(tasks.filters().unwrap()[0], a);
        assert!(matches!(
            tasks.edit_filter("aaaaaaaaaaaa", "A", "@a", ""),
            Err(Error::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn a_filter_moves_before_another_or_to_the_end() {
        let (_, tasks) = open();
        let ids: Vec<String> = ["A", "B", "C", "D"]
            .iter()
            .map(|t| tasks.create_filter(t, "@a", "").unwrap().id)
            .collect();
        tasks.move_filter(&ids[3], Some(&ids[1])).unwrap();
        assert_eq!(names(&tasks), ["A", "D", "B", "C"]);
        tasks.move_filter(&ids[3], Some(&ids[0])).unwrap();
        assert_eq!(names(&tasks), ["D", "A", "B", "C"]);
        tasks.move_filter(&ids[3], None).unwrap();
        assert_eq!(names(&tasks), ["A", "B", "C", "D"]);
        tasks.move_filter(&ids[0], Some(&ids[0])).unwrap();
        assert_eq!(names(&tasks), ["A", "B", "C", "D"]);
        let unknown = tasks
            .move_filter(&ids[0], Some("aaaaaaaaaaaa"))
            .unwrap_err();
        assert_eq!(unknown.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn a_filter_moves_among_neighbours_that_share_a_position() {
        let (files, tasks) = open();
        let ids: Vec<String> = ["A", "B", "C"]
            .iter()
            .map(|t| tasks.create_filter(t, "@a", "").unwrap().id)
            .collect();
        for id in &ids {
            let path = format!("Filters/{id}.md");
            let mut file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
            file.set_scalar("position", "1");
            files.write(Path::new(&path), &file.render()).unwrap();
        }
        let order = names(&tasks);
        let listed = tasks.filters().unwrap();
        let before = files.read(Path::new(&format!("Filters/{}.md", listed[0].id)));
        tasks
            .move_filter(&listed[2].id, Some(&listed[1].id))
            .unwrap();
        let moved = names(&tasks);
        assert_eq!(moved, [&order[0], &order[2], &order[1]].map(String::clone));
        // The Filter before the drop keeps its file.
        let after = files.read(Path::new(&format!("Filters/{}.md", listed[0].id)));
        assert_eq!(after.unwrap(), before.unwrap());
    }

    #[test]
    fn deleting_a_filter_leaves_every_task_file_untouched() {
        let (files, tasks) = open();
        let id = add(&tasks, "t", &["work"]);
        let before = files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap();
        let a = tasks.create_filter("A", "@work", "").unwrap();
        let b = tasks.create_filter("B", "@work", "").unwrap();
        tasks.delete_filter(&a.id).unwrap();
        assert_eq!(tasks.filters().unwrap(), [b]);
        assert!(!files.exists(Path::new(&format!("Filters/{}.md", a.id))));
        assert_eq!(
            files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap(),
            before
        );
        assert_eq!(
            tasks.delete_filter(&a.id).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn a_filter_counts_the_open_tasks_it_matches() {
        let (_, tasks) = open();
        add(&tasks, "one", &["work"]);
        add(&tasks, "two", &["work"]);
        add(&tasks, "later", &["work", "backlog"]);
        tasks.create_filter("Work", "@work", "").unwrap();
        tasks.create_filter("None", "@nothing", "").unwrap();
        let counts: Vec<u32> = tasks.filters().unwrap().iter().map(|f| f.matches).collect();
        assert_eq!(counts, [3, 0]);
    }

    #[test]
    fn a_filter_with_an_unreadable_query_is_listed_with_its_error_and_can_be_edited() {
        let (files, tasks) = open();
        add(&tasks, "t", &["work"]);
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        assert_eq!(filter.error, None);
        let path = format!("Filters/{}.md", filter.id);
        let mut file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        file.set_scalar("query", "@work &");
        files.write(Path::new(&path), &file.render()).unwrap();
        let listed = &tasks.filters().unwrap()[0];
        assert!(listed.error.is_some());
        assert_eq!(listed.matches, 0);
        let fixed = tasks.edit_filter(&filter.id, "Work", "@work", "").unwrap();
        assert_eq!((fixed.error, fixed.matches), (None, 1));
    }

    fn order_fixture() -> (Arc<MemoryFiles>, Tasks, Filter, Vec<String>) {
        let (files, tasks) = open();
        let ids = ["a", "b", "c", "d"]
            .map(|t| add(&tasks, t, &["work"]))
            .to_vec();
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        (files, tasks, filter, ids)
    }

    fn shown(tasks: &Tasks, filter: &Filter) -> Vec<String> {
        titles(rows_of_filter(tasks, &filter.id))
    }

    #[test]
    fn a_task_moves_before_another_in_a_filter_or_to_the_end() {
        let (_, tasks, f, ids) = order_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[1]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "d", "b", "c"]);
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[0]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["d", "a", "b", "c"]);
        tasks.move_in_filter(&f.id, None, &ids[3], None).unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "c", "d"]);
        tasks
            .move_in_filter(&f.id, None, &ids[0], Some(&ids[2]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "a", "c", "d"]);
        tasks
            .move_in_filter(&f.id, None, &ids[0], Some(&ids[0]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "a", "c", "d"]);
    }

    #[test]
    fn moving_an_unknown_task_or_before_one_not_shown_is_not_found() {
        let (_, tasks, f, ids) = order_fixture();
        let other = add(&tasks, "e", &["home"]);
        for (task, before) in [
            ("aaaaaaaaaaaa", None),
            (other.as_str(), None),
            (ids[0].as_str(), Some(other.as_str())),
        ] {
            let e = tasks.move_in_filter(&f.id, None, task, before).unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::NotFound);
        }
        let e = tasks
            .move_in_filter("aaaaaaaaaaaa", None, &ids[0], None)
            .unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn the_first_move_places_every_task_shown_in_one_write() {
        let (files, tasks, f, ids) = order_fixture();
        let path = format!("Filters/{}.md", f.id);
        assert_eq!(
            places_of(&Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap()),
            Places::new()
        );
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[0]))
            .unwrap();
        let file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        let mut placed: Vec<String> = (places_of(&file).remove(&None).unwrap())
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        placed.sort();
        let mut all = ids.clone();
        all.sort();
        assert_eq!(placed, all);
    }

    #[test]
    fn later_moves_write_the_filter_file_only_and_leave_actives_order() {
        let (files, tasks, f, ids) = order_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[0]))
            .unwrap();
        let tasks_before: Vec<String> = ids
            .iter()
            .map(|id| files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap())
            .collect();
        let other = tasks.create_filter("Other", "@work", "").unwrap();
        tasks.move_in_filter(&f.id, None, &ids[1], None).unwrap();
        assert_eq!(shown(&tasks, &f), ["d", "a", "c", "b"]);
        assert_eq!(shown(&tasks, &other), ["a", "b", "c", "d"]);
        assert_eq!(titles(tasks.rows().unwrap()), ["a", "b", "c", "d"]);
        let tasks_after: Vec<String> = ids
            .iter()
            .map(|id| files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap())
            .collect();
        assert_eq!(tasks_before, tasks_after);
    }

    #[test]
    fn tasks_without_a_place_follow_the_placed_ones_in_the_order_created() {
        let (_, tasks, f, ids) = order_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[2], Some(&ids[0]))
            .unwrap();
        add(&tasks, "e", &["work"]);
        add(&tasks, "f", &["work"]);
        assert_eq!(shown(&tasks, &f), ["c", "a", "b", "d", "e", "f"]);
    }

    #[test]
    fn unplaced_tasks_follow_the_placed_ones_by_created_then_item_id() {
        let (files, tasks) = open();
        let f = tasks.create_filter("F", "*", "").unwrap();
        // `x` and `y` tie.
        for (id, created) in [
            ("aaaaaaaaaaaa", "2024-03-01T00:00:00Z"),
            ("bbbbbbbbbbbb", "2024-01-01T00:00:00Z"),
            ("yyyyyyyyyyyy", "2024-02-01T00:00:00Z"),
            ("xxxxxxxxxxxx", "2024-02-01T00:00:00Z"),
        ] {
            files.load(
                Path::new(&format!("Tasks/{id}.md")),
                format!("---\nid: {id}\ntitle: {id}\npriority: 4\ncreated: {created}\nupdated: 2024-04-01T00:00:00Z\n---\n"),
            );
        }
        assert_eq!(
            shown(&tasks, &f),
            [
                "bbbbbbbbbbbb",
                "xxxxxxxxxxxx",
                "yyyyyyyyyyyy",
                "aaaaaaaaaaaa"
            ]
        );
        tasks
            .move_in_filter(&f.id, None, "aaaaaaaaaaaa", Some("bbbbbbbbbbbb"))
            .unwrap();
        assert_eq!(shown(&tasks, &f)[0], "aaaaaaaaaaaa");
    }

    #[test]
    fn places_of_tasks_no_longer_shown_are_dropped_so_a_returning_task_starts_at_the_end() {
        let (files, tasks, f, ids) = order_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[0]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["d", "a", "b", "c"]);
        tasks.remove_tag(&ids[3], "work").unwrap();
        // Any write of the Filter's file drops the place.
        tasks.edit_filter(&f.id, "Work", "@work", "").unwrap();
        let text = files
            .read(Path::new(&format!("Filters/{}.md", f.id)))
            .unwrap();
        assert!(!text.contains(&ids[3]));
        tasks.add_tag(&ids[3], "work").unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "c", "d"]);
    }

    #[test]
    fn a_drop_where_the_task_already_is_writes_nothing() {
        let (files, tasks, f, ids) = order_fixture();
        let path = format!("Filters/{}.md", f.id);
        let before = files.read(Path::new(&path)).unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[1], Some(&ids[2]))
            .unwrap();
        tasks.move_in_filter(&f.id, None, &ids[3], None).unwrap();
        assert_eq!(files.read(Path::new(&path)).unwrap(), before);
    }

    #[test]
    fn a_task_pending_rejected_keeps_its_place_to_be_reverted() {
        let (_, tasks, f, ids) = order_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[0]))
            .unwrap();
        tasks.close(&ids[3], crate::Closing::Rejected).unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "c"]);
        tasks.edit_filter(&f.id, "Renamed", "@work", "").unwrap();
        tasks.revert();
        assert_eq!(shown(&tasks, &f), ["d", "a", "b", "c"]);
    }

    #[test]
    fn places_that_share_a_key_leave_room_on_a_move() {
        let (files, tasks, f, ids) = order_fixture();
        let path = format!("Filters/{}.md", f.id);
        let mut file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        let tied: Vec<String> = ids.iter().map(|id| format!("{id} 1")).collect();
        file.set("places", Value::List(tied));
        files.write(Path::new(&path), &file.render()).unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "c", "d"]);
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[2]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "d", "c"]);
    }

    /// Tasks "p", "s1", "s2", "s3" (the Subtasks of p), "t1", "t2", "t3" in
    /// this order, all tagged `work`, and a Filter `@work`.
    fn subtask_fixture() -> (Arc<MemoryFiles>, Tasks, Filter, Vec<String>) {
        let (files, tasks) = open();
        let p = add(&tasks, "p", &["work"]);
        let mut ids = vec![p.clone()];
        for title in ["s1", "s2", "s3"] {
            let id = tasks.create_subtask(&p, title, 4, &[]).unwrap().id;
            ids.push(id);
        }
        for title in ["t1", "t2", "t3"] {
            ids.push(add(&tasks, title, &["work"]));
        }
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        (files, tasks, filter, ids)
    }

    fn sheet(tasks: &Tasks, parent: &str) -> Vec<String> {
        titles(tasks.subtasks(parent).unwrap())
    }

    #[test]
    fn siblings_take_the_rows_the_filter_gives_them_in_subtask_order() {
        let (_, tasks, f, ids) = subtask_fixture();
        tasks
            .move_in_filter(&f.id, None, &ids[4], Some(&ids[2]))
            .unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[5], Some(&ids[3]))
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["p", "s1", "t1", "s2", "t2", "s3", "t3"]);
        // The sheet reorders the Subtasks; the Filter keeps its rows for them.
        tasks.move_subtask(&ids[3], Some(&ids[1])).unwrap();
        assert_eq!(sheet(&tasks, &ids[0]), ["s3", "s1", "s2"]);
        assert_eq!(shown(&tasks, &f), ["p", "s3", "t1", "s1", "t2", "s2", "t3"]);
    }

    #[test]
    fn a_drag_in_a_filter_passing_a_sibling_changes_the_subtask_order_everywhere() {
        let (_, tasks, f, ids) = subtask_fixture();
        let other = tasks.create_filter("Other", "@work", "").unwrap();
        // s3 passes s2: just after the last sibling it now follows.
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[2]))
            .unwrap();
        assert_eq!(sheet(&tasks, &ids[0]), ["s1", "s3", "s2"]);
        assert_eq!(shown(&tasks, &other)[..4], ["p", "s1", "s3", "s2"]);
        // To the front: before the first sibling it now precedes.
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[1]))
            .unwrap();
        assert_eq!(sheet(&tasks, &ids[0]), ["s3", "s1", "s2"]);
        // To the end.
        tasks.move_in_filter(&f.id, None, &ids[3], None).unwrap();
        assert_eq!(sheet(&tasks, &ids[0]), ["s1", "s2", "s3"]);
        assert_eq!(shown(&tasks, &f)[6], "s3");
    }

    #[test]
    fn a_drag_passing_no_sibling_changes_only_that_filters_order() {
        let (files, tasks, f, ids) = subtask_fixture();
        let other = tasks.create_filter("Other", "@work", "").unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[4], Some(&ids[2]))
            .unwrap();
        let task = |id: &str| files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap();
        let before: Vec<_> = ids.iter().map(|id| task(id)).collect();
        tasks
            .move_in_filter(&f.id, None, &ids[2], Some(&ids[4]))
            .unwrap();
        assert_eq!(shown(&tasks, &f)[..4], ["p", "s1", "s2", "t1"]);
        assert_eq!(sheet(&tasks, &ids[0]), ["s1", "s2", "s3"]);
        assert_eq!(ids.iter().map(|id| task(id)).collect::<Vec<_>>(), before);
        assert_eq!(shown(&tasks, &other)[..4], ["p", "s1", "s2", "s3"]);
    }

    #[test]
    fn a_drag_passing_a_sibling_hidden_from_the_filter_follows_the_shown_ones() {
        let (_, tasks, _, ids) = subtask_fixture();
        tasks.add_tag(&ids[1], "x").unwrap();
        tasks.add_tag(&ids[3], "x").unwrap();
        let f = tasks.create_filter("X", "@x", "").unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[3], Some(&ids[1]))
            .unwrap();
        assert_eq!(sheet(&tasks, &ids[0]), ["s3", "s1", "s2"]);
    }

    /// "a" P3, "b" P1, "c" P3, "d" P2, "e" P1 in this Manual order, in a
    /// Filter `@work`.
    fn priority_fixture() -> (Arc<MemoryFiles>, Tasks, Filter, Vec<String>) {
        let (files, tasks) = open();
        let ids = [("a", 3), ("b", 1), ("c", 3), ("d", 2), ("e", 1)]
            .map(|(title, priority)| {
                let task = tasks.create(title, priority, &["work".into()]).unwrap();
                task.id
            })
            .to_vec();
        let filter = tasks.create_filter("Work", "@work", "").unwrap();
        (files, tasks, filter, ids)
    }

    const ASCENDING: FilterSort = FilterSort::Priority(Direction::Ascending);
    const DESCENDING: FilterSort = FilterSort::Priority(Direction::Descending);

    #[test]
    fn both_directions_sort_by_priority_with_ties_in_manual_order() {
        let (_, tasks, f, ids) = priority_fixture();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "a", "c"]);
        tasks
            .move_in_filter(&f.id, None, &ids[4], Some(&ids[1]))
            .unwrap(); // e before b
        assert_eq!(shown(&tasks, &f), ["e", "b", "d", "a", "c"]);
        tasks.set_sort(&f.id, DESCENDING).unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "c", "d", "e", "b"]); // groups flip only
        assert_eq!(tasks.stored_filters().unwrap()[0].filter.sort, DESCENDING);
    }

    #[test]
    fn a_sort_writes_no_task_file() {
        let (files, tasks, f, ids) = priority_fixture();
        let task = |id: &str| files.read(Path::new(&format!("Tasks/{id}.md"))).unwrap();
        let before: Vec<_> = ids.iter().map(|id| task(id)).collect();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[2], Some(&ids[0]))
            .unwrap();
        tasks.set_sort(&f.id, FilterSort::Manual).unwrap();
        assert_eq!(ids.iter().map(|id| task(id)).collect::<Vec<_>>(), before);
    }

    #[test]
    fn new_newly_matching_and_reprioritised_tasks_take_their_sorted_place() {
        let (_, tasks, f, ids) = priority_fixture();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        add_priority(&tasks, "f", 3, &["work"]);
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "a", "c", "f"]);
        let x = add_priority(&tasks, "x", 1, &[]);
        tasks.add_tag(&x, "work").unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "e", "x", "d", "a", "c", "f"]);
        tasks.edit(&ids[2], "c", "", 1, &[]).unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "c", "e", "x", "d", "a", "f"]);
    }

    fn add_priority(tasks: &Tasks, title: &str, priority: u8, tags: &[&str]) -> String {
        let tags: Vec<String> = tags.iter().map(|t| (*t).to_owned()).collect();
        tasks.create(title, priority, &tags).unwrap().id
    }

    #[test]
    fn a_drag_within_the_priority_group_keeps_the_sort() {
        let (_, tasks, f, ids) = priority_fixture();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[2], Some(&ids[0]))
            .unwrap(); // c before a
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "c", "a"]);
        tasks.move_in_filter(&f.id, None, &ids[2], None).unwrap(); // the end, still P3
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "a", "c"]);
        assert_eq!(tasks.stored_filters().unwrap()[0].filter.sort, ASCENDING);
    }

    #[test]
    fn a_drag_across_groups_keeps_the_order_shown_and_turns_manual_in_one_write() {
        let (files, tasks, f, ids) = priority_fixture();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        files.take_changes();
        tasks
            .move_in_filter(&f.id, None, &ids[0], Some(&ids[1]))
            .unwrap(); // a before b
        assert_eq!(files.take_changes().len(), 1);
        assert_eq!(shown(&tasks, &f), ["a", "b", "e", "d", "c"]);
        assert_eq!(
            tasks.stored_filters().unwrap()[0].filter.sort,
            FilterSort::Manual
        );
        tasks.set_sort(&f.id, DESCENDING).unwrap();
        tasks.move_in_filter(&f.id, None, &ids[1], None).unwrap(); // P1 b to the end
        assert_eq!(shown(&tasks, &f), ["a", "c", "d", "e", "b"]);
        assert_eq!(tasks.stored_filters().unwrap()[0].filter.sort, DESCENDING);
        tasks
            .move_in_filter(&f.id, None, &ids[1], Some(&ids[3]))
            .unwrap(); // b before d
        assert_eq!(shown(&tasks, &f), ["a", "c", "b", "d", "e"]);
        assert_eq!(
            tasks.stored_filters().unwrap()[0].filter.sort,
            FilterSort::Manual
        );
    }

    #[test]
    fn picking_manual_keeps_the_order_shown_and_picking_priority_clears_it() {
        let (_, tasks, f, ids) = priority_fixture();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        tasks.set_sort(&f.id, FilterSort::Manual).unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "a", "c"]);
        tasks
            .move_in_filter(&f.id, None, &ids[4], Some(&ids[1]))
            .unwrap(); // e before b
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        assert_eq!(shown(&tasks, &f), ["b", "e", "d", "a", "c"]); // places gone
        assert!(tasks.stored_filters().unwrap()[0].places.is_empty());
    }

    #[test]
    fn sorted_subtasks_take_their_own_priority_but_siblings_keep_their_subtask_order() {
        let (_, tasks, _, _) = priority_fixture();
        let p = add_priority(&tasks, "p", 4, &["work"]);
        tasks.create_subtask(&p, "s1", 3, &["work".into()]).unwrap();
        tasks.create_subtask(&p, "s2", 1, &["work".into()]).unwrap();
        let f = tasks.create_filter("Work", "@work", "").unwrap();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        // The rows of s1 (P3) and s2 (P1) are refilled in Subtask order.
        assert_eq!(
            shown(&tasks, &f),
            ["b", "e", "s1", "d", "a", "c", "s2", "p"]
        );
    }

    #[test]
    fn an_unknown_sort_reads_as_manual_and_survives_a_write() {
        let (files, tasks, f, ids) = priority_fixture();
        let path = format!("Filters/{}.md", f.id);
        let text = files.read(Path::new(&path)).unwrap();
        files
            .write(
                Path::new(&path),
                &text.replace("favorite:", "sort: created ascending\nfavorite:"),
            )
            .unwrap();
        assert_eq!(shown(&tasks, &f), ["a", "b", "c", "d", "e"]);
        tasks.set_favorite(&f.id, true).unwrap();
        tasks
            .move_in_filter(&f.id, None, &ids[4], Some(&ids[0]))
            .unwrap();
        let file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        assert_eq!(file.scalar("sort"), Some("created ascending"));
    }

    #[test]
    fn the_menu_keys_and_their_directions() {
        assert_eq!(
            FilterSortKey::ALL.map(|k| FilterSort::Manual.with_key(k)),
            [FilterSort::Manual, ASCENDING]
        );
        assert_eq!(DESCENDING.with_key(FilterSortKey::Priority), DESCENDING);
        assert_eq!(
            DESCENDING.with_key(FilterSortKey::Manual).key(),
            FilterSortKey::Manual
        );
    }

    /// The Filter's Groups as (Tag, Titles).
    fn grouped(tasks: &Tasks, id: &str) -> Vec<(Option<String>, Vec<String>)> {
        let groups = tasks.filter_groups(id).unwrap();
        (groups.into_iter())
            .map(|g| (g.tag, titles(g.rows)))
            .collect()
    }

    fn group(tag: Option<&str>, titles: &[&str]) -> (Option<String>, Vec<String>) {
        (
            tag.map(str::to_owned),
            titles.iter().map(|t| (*t).to_owned()).collect(),
        )
    }

    /// "x" @work, "y" @private, "z" no Tag, "xy" both; in a Filter `*`
    /// Grouped by `@work @private`.
    fn group_fixture() -> (Arc<MemoryFiles>, Tasks, Filter) {
        let (files, tasks) = open();
        add(&tasks, "x", &["work"]);
        add(&tasks, "y", &["private"]);
        add(&tasks, "z", &[]);
        add(&tasks, "xy", &["private", "work"]);
        let f = tasks.create_filter("All", "*", "@work @private").unwrap();
        (files, tasks, f)
    }

    #[test]
    fn groups_follow_the_group_by_order_after_the_tasks_without_a_group() {
        let (_, tasks, f) = group_fixture();
        assert_eq!(
            grouped(&tasks, &f.id),
            [
                group(None, &["z"]),
                group(Some("work"), &["x", "xy"]),
                group(Some("private"), &["y", "xy"]),
            ]
        );
        assert_eq!(f.group_by, ["work", "private"]);
    }

    #[test]
    fn an_empty_group_keeps_its_heading_even_if_the_query_excludes_its_tag() {
        let (_, tasks) = open();
        add(&tasks, "a", &["work"]);
        add(&tasks, "b", &["home"]);
        let f = tasks.create_filter("F", "!@home", "@home").unwrap();
        assert_eq!(
            grouped(&tasks, &f.id),
            [group(None, &["a"]), group(Some("home"), &[])]
        );
    }

    #[test]
    fn a_subtask_is_grouped_by_its_own_tags_and_keeps_its_subtask_order() {
        let (_, tasks) = open();
        let parent = add(&tasks, "p", &["a"]);
        tasks
            .create_subtask(&parent, "s1", 4, &["b".into()])
            .unwrap();
        tasks.create_subtask(&parent, "s2", 4, &[]).unwrap();
        tasks
            .create_subtask(&parent, "s3", 4, &["b".into()])
            .unwrap();
        let f = tasks.create_filter("F", "*", "@b").unwrap();
        assert_eq!(
            grouped(&tasks, &f.id),
            [group(None, &["p", "s2"]), group(Some("b"), &["s1", "s3"])]
        );
    }

    #[test]
    fn sort_applies_within_each_group() {
        let (_, tasks) = open();
        let tag = vec!["w".to_owned()];
        tasks.create("low", 4, &tag).unwrap();
        tasks.create("high", 1, &tag).unwrap();
        tasks.create("plain low", 4, &[]).unwrap();
        tasks.create("plain high", 1, &[]).unwrap();
        let f = tasks.create_filter("F", "*", "@w").unwrap();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        assert_eq!(
            grouped(&tasks, &f.id),
            [
                group(None, &["plain high", "plain low"]),
                group(Some("w"), &["high", "low"]),
            ]
        );
    }

    #[test]
    fn closing_a_task_removes_it_from_every_group_and_revert_brings_it_back() {
        let (_, tasks, f) = group_fixture();
        let xy = tasks
            .rows()
            .unwrap()
            .into_iter()
            .find(|r| r.task.title == "xy");
        let xy = xy.unwrap().task.id;
        tasks.close(&xy, crate::Closing::Rejected).unwrap();
        assert_eq!(
            grouped(&tasks, &f.id),
            [
                group(None, &["z"]),
                group(Some("work"), &["x"]),
                group(Some("private"), &["y"])
            ]
        );
        tasks.revert();
        assert_eq!(grouped(&tasks, &f.id)[1], group(Some("work"), &["x", "xy"]));
        assert_eq!(
            grouped(&tasks, &f.id)[2],
            group(Some("private"), &["y", "xy"])
        );
    }

    #[test]
    fn group_by_is_a_scalar_of_tags_and_absent_without_one() {
        let (files, tasks, f) = group_fixture();
        let path = format!("Filters/{}.md", f.id);
        let file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        assert_eq!(file.scalar("group-by"), Some("work private"));
        tasks.edit_filter(&f.id, "All", "*", "  ").unwrap();
        let file = Document::parse(&files.read(Path::new(&path)).unwrap()).unwrap();
        assert_eq!(file.scalar("group-by"), None);
        assert_eq!(
            grouped(&tasks, &f.id),
            [group(None, &["x", "y", "z", "xy"])]
        );
    }

    #[test]
    fn group_by_text_drops_repeats_and_at_signs() {
        let (_, tasks, f) = group_fixture();
        let edited = tasks
            .edit_filter(&f.id, "All", "*", "@private work  @private")
            .unwrap();
        assert_eq!(edited.group_by, ["private", "work"]);
    }

    #[test]
    fn an_unknown_tag_a_wildcard_or_other_text_is_refused_and_saves_nothing() {
        let (files, tasks, f) = group_fixture();
        let path = format!("Filters/{}.md", f.id);
        let before = files.read(Path::new(&path)).unwrap();
        for (text, unknown) in [
            ("@work @nope", true),
            ("@wo*", false),
            ("@Work", false),
            ("@", false),
        ] {
            let refused = tasks.edit_filter(&f.id, "Other", "*", text);
            match refused {
                Err(Error::UnknownGroupByTag(_)) if unknown => {}
                Err(Error::InvalidGroupByTag(_)) if !unknown => {}
                other => panic!("{text}: {other:?}"),
            }
            assert!(tasks.check_group_by(text).is_err());
        }
        assert!(matches!(
            tasks.create_filter("New", "*", "@nope"),
            Err(Error::UnknownGroupByTag(_))
        ));
        assert_eq!(files.read(Path::new(&path)).unwrap(), before);
        assert_eq!(files.list(Path::new("Filters")).unwrap().len(), 1);
        assert_eq!(
            tasks.check_group_by("@work private").unwrap(),
            ["work", "private"]
        );
    }

    /// The ID of the Task titled `title`.
    fn id_of(tasks: &Tasks, title: &str) -> String {
        let rows = tasks.rows().unwrap();
        rows.into_iter()
            .find(|r| r.task.title == title)
            .unwrap()
            .task
            .id
    }

    fn drag(tasks: &Tasks, f: &Filter, group: Option<&str>, task: &str, before: Option<&str>) {
        let (task, before) = (id_of(tasks, task), before.map(|b| id_of(tasks, b)));
        tasks
            .move_in_filter(&f.id, group, &task, before.as_deref())
            .unwrap();
    }

    #[test]
    fn a_drag_in_one_group_leaves_the_same_task_in_its_other_groups() {
        let (files, tasks, f) = group_fixture();
        drag(&tasks, &f, Some("work"), "xy", Some("x"));
        assert_eq!(grouped(&tasks, &f.id)[1], group(Some("work"), &["xy", "x"]));
        assert_eq!(
            grouped(&tasks, &f.id)[2],
            group(Some("private"), &["y", "xy"])
        );
        let text = files
            .read(Path::new(&format!("Filters/{}.md", f.id)))
            .unwrap();
        let file = Document::parse(&text).unwrap();
        let entries = file.list("places").unwrap();
        assert!(
            entries.iter().all(|e| e.starts_with("work ")),
            "{entries:?}"
        );
    }

    #[test]
    fn a_drag_among_the_tasks_without_a_group_orders_them() {
        let (_, tasks) = open();
        add(&tasks, "a", &[]);
        add(&tasks, "b", &[]);
        add(&tasks, "w", &["work"]);
        let f = tasks.create_filter("F", "*", "@work").unwrap();
        drag(&tasks, &f, None, "b", Some("a"));
        assert_eq!(
            grouped(&tasks, &f.id),
            [group(None, &["b", "a"]), group(Some("work"), &["w"])]
        );
    }

    #[test]
    fn a_drag_with_a_group_that_doesnt_show_the_task_is_not_found() {
        let (_, tasks, f) = group_fixture();
        let (x, y, z) = (id_of(&tasks, "x"), id_of(&tasks, "y"), id_of(&tasks, "z"));
        for (group, task, before) in [
            (Some("private"), &x, None),
            (Some("work"), &x, Some(&y)),
            (None, &x, None),
            (Some("home"), &x, None),
            (Some("work"), &z, None),
        ] {
            let e = tasks
                .move_in_filter(&f.id, group, task, before.map(String::as_str))
                .unwrap_err();
            assert_eq!(e.kind(), io::ErrorKind::NotFound, "{group:?}");
        }
    }

    #[test]
    fn a_drop_among_one_priority_in_a_group_keeps_the_sort_and_any_other_turns_manual() {
        let (_, tasks) = open();
        let tag = vec!["w".to_owned()];
        tasks.create("p1", 1, &tag).unwrap();
        tasks.create("a4", 4, &tag).unwrap();
        tasks.create("b4", 4, &tag).unwrap();
        tasks.create("plain1", 1, &[]).unwrap();
        tasks.create("plain4", 4, &[]).unwrap();
        let f = tasks.create_filter("F", "*", "@w").unwrap();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        drag(&tasks, &f, Some("w"), "b4", Some("a4"));
        assert_eq!(tasks.filters().unwrap()[0].sort, ASCENDING);
        assert_eq!(
            grouped(&tasks, &f.id)[1],
            group(Some("w"), &["p1", "b4", "a4"])
        );
        drag(&tasks, &f, Some("w"), "b4", Some("p1"));
        assert_eq!(tasks.filters().unwrap()[0].sort, FilterSort::Manual);
        assert_eq!(
            grouped(&tasks, &f.id),
            [
                group(None, &["plain1", "plain4"]),
                group(Some("w"), &["b4", "p1", "a4"])
            ]
        );
    }

    #[test]
    fn manual_from_priority_keeps_the_order_shown_in_every_group() {
        let (_, tasks) = open();
        let tag = vec!["w".to_owned()];
        tasks.create("low", 4, &tag).unwrap();
        tasks.create("high", 1, &tag).unwrap();
        tasks.create("plain low", 4, &[]).unwrap();
        tasks.create("plain high", 1, &[]).unwrap();
        let f = tasks.create_filter("F", "*", "@w").unwrap();
        tasks.set_sort(&f.id, ASCENDING).unwrap();
        tasks.set_sort(&f.id, FilterSort::Manual).unwrap();
        assert_eq!(
            grouped(&tasks, &f.id),
            [
                group(None, &["plain high", "plain low"]),
                group(Some("w"), &["high", "low"]),
            ]
        );
    }

    #[test]
    fn a_new_group_starts_in_the_order_shown_before_and_one_taken_out_starts_over() {
        let (_, tasks) = open();
        add(&tasks, "a", &["w"]);
        add(&tasks, "b", &["w"]);
        add(&tasks, "c", &["w"]);
        let f = tasks.create_filter("F", "*", "").unwrap();
        drag(&tasks, &f, None, "c", Some("a"));
        tasks.edit_filter(&f.id, "F", "*", "@w").unwrap();
        assert_eq!(
            grouped(&tasks, &f.id)[1],
            group(Some("w"), &["c", "a", "b"])
        );
        drag(&tasks, &f, Some("w"), "b", Some("c"));
        tasks.edit_filter(&f.id, "F", "*", "").unwrap();
        tasks.edit_filter(&f.id, "F", "*", "@w").unwrap();
        // Started over: neither the Group's drag nor the old Filter order stays.
        assert_eq!(
            grouped(&tasks, &f.id)[1],
            group(Some("w"), &["a", "b", "c"])
        );
    }

    #[test]
    fn places_go_when_a_task_leaves_its_group_or_the_group_leaves_group_by() {
        let (files, tasks, f) = group_fixture();
        drag(&tasks, &f, Some("work"), "xy", Some("x"));
        drag(&tasks, &f, Some("private"), "xy", Some("y"));
        let places = || {
            let text = files
                .read(Path::new(&format!("Filters/{}.md", f.id)))
                .unwrap();
            places_of(&Document::parse(&text).unwrap())
        };
        let xy = id_of(&tasks, "xy");
        tasks.remove_tag(&xy, "private").unwrap();
        drag(&tasks, &f, Some("work"), "xy", None); // a write of the Filter's file
        let places = places();
        assert!(
            places[&Some("work".to_owned())]
                .iter()
                .any(|(id, _)| *id == xy)
        );
        assert!(
            places[&Some("private".to_owned())]
                .iter()
                .all(|(id, _)| *id != xy)
        );
        tasks.edit_filter(&f.id, "All", "*", "@private").unwrap();
        assert!(!places_of_group(&files, &f, "work"));
    }

    #[test]
    fn deleting_a_tag_takes_it_out_of_every_group_by_with_its_places() {
        let (files, tasks, f) = group_fixture();
        drag(&tasks, &f, Some("work"), "xy", Some("x"));
        let other = tasks.create_filter("O", "*", "@private @work").unwrap();
        tasks.delete_tag("work").unwrap();
        let listed = tasks.filters().unwrap();
        assert_eq!(listed[0].group_by, ["private"]);
        assert_eq!(listed[1].group_by, ["private"]);
        assert!(!places_of_group(&files, &f, "work"));
        assert!(
            grouped(&tasks, &other.id)
                .iter()
                .all(|g| g.0.as_deref() != Some("work"))
        );
        assert!(!tasks.tags().contains("work"));
        tasks.delete_tag("work").unwrap(); // safe to repeat
    }

    #[test]
    fn a_group_by_tag_missing_from_the_tags_file_returns_grey_when_filters_are_read() {
        let (_, tasks, _) = group_fixture();
        tasks.set_tag_color("work", Some("#d0021b")).unwrap();
        tasks.forget_tag("work").unwrap(); // a Delete racing the edit
        tasks.filters().unwrap();
        assert!(tasks.tags().contains("work"));
        assert_eq!(
            tasks
                .tags()
                .list()
                .iter()
                .find(|t| t.name == "work")
                .unwrap()
                .color,
            None
        );
    }

    fn places_of_group(files: &MemoryFiles, f: &Filter, tag: &str) -> bool {
        let text = files
            .read(Path::new(&format!("Filters/{}.md", f.id)))
            .unwrap();
        places_of(&Document::parse(&text).unwrap()).contains_key(&Some(tag.to_owned()))
    }

    #[test]
    fn revert_puts_a_task_back_at_its_old_place_in_each_group() {
        let (_, tasks, f) = group_fixture();
        drag(&tasks, &f, Some("work"), "xy", Some("x"));
        drag(&tasks, &f, Some("private"), "xy", Some("y"));
        let xy = id_of(&tasks, "xy");
        tasks.close(&xy, crate::Closing::Rejected).unwrap();
        tasks
            .edit_filter(&f.id, "Renamed", "*", "@work @private")
            .unwrap();
        tasks.revert();
        let groups = grouped(&tasks, &f.id);
        assert_eq!(groups[1], group(Some("work"), &["xy", "x"]));
        assert_eq!(groups[2], group(Some("private"), &["xy", "y"]));
    }
}
