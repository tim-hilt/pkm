//! Markup: the Highlights and Comments a user puts on a Book or an Article,
//! how they are stored as JSON, and how two Devices' Markup merge. Shared by
//! Books and Bookmarks (ADR-0034); each Context brings its own anchors, the
//! places in its text (`Located`).
//!
//! A Comment on a Highlight is never empty: emptying it deletes it. A page
//! Comment is its text: emptying it deletes the page Comment, and one without
//! text, which only exists between adding it and typing, isn't listed.
//!
//! Next to the Markup goes `MarkupChanges`: when each part last changed and
//! what was deleted, so that a merge never brings back what the user removed.
//! Every edit goes through a `Markup` method that records it there.

use crate::json::{Value, number, object, string};
use std::collections::BTreeMap;
use std::fmt::Debug;

/// A place in a Context's text, as stored in the Markup's JSON.
pub trait Located: Clone + PartialEq + Debug {
    fn to_json(&self) -> Value;
    /// The place in `json`; none if it is incomplete.
    fn from_json(json: &Value) -> Option<Self>;
}

/// A color in sRGB, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// `0xRRGGBB`.
    pub fn packed(self) -> u64 {
        u64::from(self.r) << 16 | u64::from(self.g) << 8 | u64::from(self.b)
    }

    /// The color of `0xRRGGBB`; none if `value` has more than 24 bits.
    pub fn unpacked(value: u64) -> Option<Self> {
        let [_, _, _, _, _, r, g, b] = value.to_be_bytes();
        (value <= 0xFF_FFFF).then_some(Self { r, g, b })
    }
}

/// A passage of text marked in a color, anchored by `A`.
#[derive(Debug, Clone, PartialEq)]
pub struct Highlight<A> {
    /// Unique among all Highlights.
    pub id: String,
    pub anchor: A,
    /// The whole passage; the fallback of the anchor.
    pub quoted_text: String,
    pub color: Rgb,
    /// When it was added, in milliseconds since the Unix epoch.
    pub created: u64,
    /// What the user wrote about the passage. Never empty.
    pub comment: Option<String>,
}

/// A Comment on a spot, `P`, belonging to no passage.
#[derive(Debug, Clone, PartialEq)]
pub struct PageComment<P> {
    /// Unique among all page Comments.
    pub id: String,
    pub place: P,
    /// When it was added, in milliseconds since the Unix epoch.
    pub created: u64,
    /// What the user wrote. Never empty in the Markup.
    pub text: String,
}

/// Everything the user added on top of a text.
#[derive(Debug, Clone, PartialEq)]
pub struct Markup<A, P> {
    /// Oldest first.
    pub highlights: Vec<Highlight<A>>,
    /// Oldest first.
    pub page_comments: Vec<PageComment<P>>,
}

// Not derived: that would demand `A: Default` and `P: Default`.
impl<A, P> Default for Markup<A, P> {
    fn default() -> Self {
        Self {
            highlights: Vec::new(),
            page_comments: Vec::new(),
        }
    }
}

/// When the parts of a Markup last changed, and what was deleted, for merging.
/// Times are milliseconds since the Unix epoch; an absent time means "before
/// this Device recorded times", which loses to any recorded one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarkupChanges {
    highlight_colors: BTreeMap<String, u64>,
    highlight_comments: BTreeMap<String, u64>,
    /// Highlight Comments that were deleted, and when.
    deleted_highlight_comments: BTreeMap<String, u64>,
    page_comment_texts: BTreeMap<String, u64>,
    /// Highlights that were removed, and when.
    deleted_highlights: BTreeMap<String, u64>,
    /// Page Comments that were removed, or emptied, and when.
    deleted_page_comments: BTreeMap<String, u64>,
}

impl MarkupChanges {
    pub fn is_empty(&self) -> bool {
        self.maps().iter().all(|(_, times)| times.is_empty())
    }

    fn maps(&self) -> [(&'static str, &BTreeMap<String, u64>); 6] {
        [
            ("highlight_colors", &self.highlight_colors),
            ("highlight_comments", &self.highlight_comments),
            (
                "deleted_highlight_comments",
                &self.deleted_highlight_comments,
            ),
            ("page_comment_texts", &self.page_comment_texts),
            ("deleted_highlights", &self.deleted_highlights),
            ("deleted_page_comments", &self.deleted_page_comments),
        ]
    }

    /// These changes and another Device's, as one: the latest time of each.
    pub fn merged(&self, other: &Self) -> Self {
        let latest = |a: &BTreeMap<String, u64>, b: &BTreeMap<String, u64>| {
            let mut latest = a.clone();
            for (id, at) in b {
                latest
                    .entry(id.clone())
                    .and_modify(|known| *known = (*known).max(*at))
                    .or_insert(*at);
            }
            latest
        };
        Self {
            highlight_colors: latest(&self.highlight_colors, &other.highlight_colors),
            highlight_comments: latest(&self.highlight_comments, &other.highlight_comments),
            deleted_highlight_comments: latest(
                &self.deleted_highlight_comments,
                &other.deleted_highlight_comments,
            ),
            page_comment_texts: latest(&self.page_comment_texts, &other.page_comment_texts),
            deleted_highlights: latest(&self.deleted_highlights, &other.deleted_highlights),
            deleted_page_comments: latest(
                &self.deleted_page_comments,
                &other.deleted_page_comments,
            ),
        }
    }

    /// The members to put into the JSON object that holds the changes.
    pub fn members(&self) -> Vec<(&'static str, Value)> {
        self.maps()
            .into_iter()
            .map(|(key, times)| {
                let times = times.iter().map(|(id, at)| (id.as_str(), number(*at)));
                (key, object(times))
            })
            .collect()
    }

    /// The changes in the object `json`; what is missing or unreadable is none.
    pub fn from_json(json: &Value) -> Self {
        let times = |key: &str| -> BTreeMap<String, u64> {
            json.get(key)
                .and_then(Value::as_object)
                .into_iter()
                .flat_map(|times| times.iter())
                .filter_map(|(id, at)| Some((id.to_owned(), at.as_u64()?)))
                .collect()
        };
        Self {
            highlight_colors: times("highlight_colors"),
            highlight_comments: times("highlight_comments"),
            deleted_highlight_comments: times("deleted_highlight_comments"),
            page_comment_texts: times("page_comment_texts"),
            deleted_highlights: times("deleted_highlights"),
            deleted_page_comments: times("deleted_page_comments"),
        }
    }
}

fn time_of(times: &BTreeMap<String, u64>, id: &str) -> u64 {
    times.get(id).copied().unwrap_or(0)
}

/// The later of two values; a tie goes to the smaller value, so the result
/// doesn't depend on the order the two sides are merged in.
fn later<T: Ord>(a: T, at_a: u64, b: T, at_b: u64) -> T {
    match at_a.cmp(&at_b) {
        std::cmp::Ordering::Greater => a,
        std::cmp::Ordering::Less => b,
        std::cmp::Ordering::Equal => a.min(b),
    }
}

impl<A: Located, P: Located> Markup<A, P> {
    /// The Markup as the Reader lists it: oldest first, without the page
    /// Comments that have no text yet.
    pub fn listed(&self) -> Self {
        let mut markup = Self {
            highlights: self.highlights.clone(),
            page_comments: self
                .page_comments
                .iter()
                .filter(|c| !c.text.is_empty())
                .cloned()
                .collect(),
        };
        markup.sort();
        markup
    }

    fn sort(&mut self) {
        self.highlights
            .sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
        self.page_comments
            .sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
    }

    /// Sets the color of Highlight `id` at `at`; nothing if there is none.
    pub fn set_highlight_color(
        &mut self,
        changes: &mut MarkupChanges,
        id: &str,
        color: Rgb,
        at: u64,
    ) {
        if let Some(h) = self.highlights.iter_mut().find(|h| h.id == id) {
            h.color = color;
            changes.highlight_colors.insert(id.to_owned(), at);
        }
    }

    /// Sets the Comment of Highlight `id` at `at`; empty `text` deletes it.
    /// Nothing if there is no such Highlight.
    pub fn set_highlight_comment(
        &mut self,
        changes: &mut MarkupChanges,
        id: &str,
        text: &str,
        at: u64,
    ) {
        let Some(h) = self.highlights.iter_mut().find(|h| h.id == id) else {
            return;
        };
        h.comment = (!text.is_empty()).then(|| text.to_owned());
        changes.highlight_comments.insert(id.to_owned(), at);
        if text.is_empty() {
            changes.deleted_highlight_comments.insert(id.to_owned(), at);
        } else {
            changes.deleted_highlight_comments.remove(id);
        }
    }

    /// Removes Highlight `id` at `at`, with its Comment.
    pub fn remove_highlight(&mut self, changes: &mut MarkupChanges, id: &str, at: u64) {
        self.highlights.retain(|h| h.id != id);
        changes.highlight_colors.remove(id);
        changes.highlight_comments.remove(id);
        changes.deleted_highlight_comments.remove(id);
        changes.deleted_highlights.insert(id.to_owned(), at);
    }

    /// Sets the text of page Comment `id` at `at`; empty `text` deletes it.
    pub fn set_page_comment_text(
        &mut self,
        changes: &mut MarkupChanges,
        id: &str,
        text: &str,
        at: u64,
    ) {
        if text.is_empty() {
            self.page_comments.retain(|c| c.id != id);
        } else if let Some(c) = self.page_comments.iter_mut().find(|c| c.id == id) {
            c.text = text.to_owned();
        }
        if self.page_comments.iter().any(|c| c.id == id) {
            changes.page_comment_texts.insert(id.to_owned(), at);
        } else {
            changes.deleted_page_comments.insert(id.to_owned(), at);
        }
    }

    /// Removes page Comment `id` at `at`.
    pub fn remove_page_comment(&mut self, changes: &mut MarkupChanges, id: &str, at: u64) {
        self.page_comments.retain(|c| c.id != id);
        changes.page_comment_texts.remove(id);
        changes.deleted_page_comments.insert(id.to_owned(), at);
    }

    /// The Markup as JSON, as `from_json` reads it.
    pub fn to_json(&self) -> Value {
        let highlights = self.highlights.iter().map(|h| {
            let mut members = vec![
                ("id", string(&h.id)),
                ("anchor", h.anchor.to_json()),
                ("quoted_text", string(&h.quoted_text)),
                ("color", number(h.color.packed())),
                ("created", number(h.created)),
            ];
            if let Some(comment) = &h.comment {
                members.push(("comment", string(comment)));
            }
            object(members)
        });
        let page_comments = self.page_comments.iter().map(|c| {
            object([
                ("id", string(&c.id)),
                ("place", c.place.to_json()),
                ("created", number(c.created)),
                ("text", string(&c.text)),
            ])
        });
        object([
            ("highlights", Value::Array(highlights.collect())),
            ("page_comments", Value::Array(page_comments.collect())),
        ])
    }

    /// The Markup in `json`; entries that are incomplete are skipped.
    pub fn from_json(json: &Value) -> Self {
        let list = |key: &str| {
            json.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        };
        Self {
            highlights: list("highlights").filter_map(read_highlight).collect(),
            page_comments: list("page_comments")
                .filter_map(read_page_comment)
                .collect(),
        }
    }

    /// This Markup and `other`, the same text's Markup on another Device, as
    /// one, `mine` and `theirs` being what each side recorded: united by id,
    /// each change going to the later one, deletions kept. Commutative and
    /// idempotent.
    pub fn merged(&self, mine: &MarkupChanges, other: &Self, theirs: &MarkupChanges) -> Self {
        let mut merged = Self {
            highlights: union(
                &self.highlights,
                &other.highlights,
                |h| &h.id,
                |id| {
                    mine.deleted_highlights.contains_key(id)
                        || theirs.deleted_highlights.contains_key(id)
                },
                |a, b| merge_highlight(a, mine, b, theirs),
            ),
            page_comments: union(
                &self.page_comments,
                &other.page_comments,
                |c| &c.id,
                |id| {
                    mine.deleted_page_comments.contains_key(id)
                        || theirs.deleted_page_comments.contains_key(id)
                },
                |a, b| merge_comment(a, mine, b, theirs),
            ),
        };
        merged.sort();
        merged
    }
}

/// The entries of `a` and `b` by id, minus those `deleted`; an id on both
/// sides is `merge`d.
fn union<T: Clone>(
    a: &[T],
    b: &[T],
    id: impl Fn(&T) -> &String,
    deleted: impl Fn(&str) -> bool,
    merge: impl Fn(&T, &T) -> T,
) -> Vec<T> {
    let mut ids: Vec<&String> = a.iter().chain(b).map(&id).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter(|wanted| !deleted(wanted))
        .map(|wanted| {
            let find = |side: &[T]| side.iter().find(|t| id(t) == wanted).cloned();
            match (find(a), find(b)) {
                (Some(left), Some(right)) => merge(&left, &right),
                (Some(one), None) | (None, Some(one)) => one,
                (None, None) => unreachable!("the id came from one of them"),
            }
        })
        .collect()
}

fn merge_highlight<A: Located>(
    a: &Highlight<A>,
    left: &MarkupChanges,
    b: &Highlight<A>,
    right: &MarkupChanges,
) -> Highlight<A> {
    // A key that orders a Highlight's unchanging parts, for a deterministic tie.
    let key = |h: &Highlight<A>| format!("{:?}|{}|{}", h.anchor, h.quoted_text, h.created);
    let mut merged = if key(a) <= key(b) {
        a.clone()
    } else {
        b.clone()
    };
    let color = later(
        a.color.packed(),
        time_of(&left.highlight_colors, &a.id),
        b.color.packed(),
        time_of(&right.highlight_colors, &b.id),
    );
    merged.color = Rgb::unpacked(color).unwrap_or(merged.color);
    let comment = |h: &Highlight<A>, changes: &MarkupChanges| {
        let set = time_of(&changes.highlight_comments, &h.id);
        let deleted = time_of(&changes.deleted_highlight_comments, &h.id);
        if deleted > set {
            (None, deleted)
        } else {
            (h.comment.clone(), set)
        }
    };
    let (left_comment, left_at) = comment(a, left);
    let (right_comment, right_at) = comment(b, right);
    merged.comment = later(left_comment, left_at, right_comment, right_at);
    merged
}

fn merge_comment<P: Located>(
    a: &PageComment<P>,
    left: &MarkupChanges,
    b: &PageComment<P>,
    right: &MarkupChanges,
) -> PageComment<P> {
    let key = |c: &PageComment<P>| format!("{:?}|{}", c.place, c.created);
    let mut merged = if key(a) <= key(b) {
        a.clone()
    } else {
        b.clone()
    };
    merged.text = later(
        a.text.clone(),
        time_of(&left.page_comment_texts, &a.id),
        b.text.clone(),
        time_of(&right.page_comment_texts, &b.id),
    );
    merged
}

fn read_highlight<A: Located>(json: &Value) -> Option<Highlight<A>> {
    Some(Highlight {
        id: json.get("id")?.as_str()?.to_owned(),
        anchor: A::from_json(json.get("anchor")?)?,
        quoted_text: json.get("quoted_text")?.as_str()?.to_owned(),
        color: Rgb::unpacked(json.get("color")?.as_u64()?)?,
        created: json.get("created")?.as_u64()?,
        comment: json
            .get("comment")
            .and_then(Value::as_str)
            .filter(|c| !c.is_empty())
            .map(str::to_owned),
    })
}

fn read_page_comment<P: Located>(json: &Value) -> Option<PageComment<P>> {
    Some(PageComment {
        id: json.get("id")?.as_str()?.to_owned(),
        place: P::from_json(json.get("place")?)?,
        created: json.get("created")?.as_u64()?,
        text: json.get("text")?.as_str()?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json::parse;

    /// A place for the tests: a span of characters.
    #[derive(Debug, Clone, PartialEq)]
    struct Span(u64, u64);

    impl Located for Span {
        fn to_json(&self) -> Value {
            object([("start", number(self.0)), ("end", number(self.1))])
        }
        fn from_json(json: &Value) -> Option<Self> {
            Some(Self(
                json.get("start")?.as_u64()?,
                json.get("end")?.as_u64()?,
            ))
        }
    }

    type Test = Markup<Span, Span>;

    const YELLOW: Rgb = Rgb {
        r: 255,
        g: 235,
        b: 60,
    };
    const RED: Rgb = Rgb { r: 255, g: 0, b: 0 };

    fn highlight(id: &str, created: u64) -> Highlight<Span> {
        Highlight {
            id: id.into(),
            anchor: Span(1, 5),
            quoted_text: "quoted — “text”".into(),
            color: YELLOW,
            created,
            comment: None,
        }
    }

    fn page_comment(id: &str, created: u64, text: &str) -> PageComment<Span> {
        PageComment {
            id: id.into(),
            place: Span(7, 7),
            created,
            text: text.into(),
        }
    }

    fn read(json: &str) -> Test {
        Markup::from_json(&parse(json).unwrap())
    }

    fn with(highlights: Vec<Highlight<Span>>, page_comments: Vec<PageComment<Span>>) -> Test {
        Markup {
            highlights,
            page_comments,
        }
    }

    #[test]
    fn markup_reads_back_as_it_was_written() {
        let mut commented = highlight("a", 1);
        commented.comment = Some("why\nnot".into());
        let markup = with(
            vec![commented, highlight("b", 2)],
            vec![page_comment("c", 3, "spot")],
        );
        assert_eq!(read(&markup.to_json().to_json()), markup);
    }

    #[test]
    fn colors_keep_all_their_channels() {
        let mut h = highlight("a", 1);
        h.color = Rgb { r: 1, g: 2, b: 255 };
        let markup = with(vec![h], vec![]);
        assert_eq!(read(&markup.to_json().to_json()), markup);
        assert_eq!(Rgb::unpacked(0x1_00_00_00), None);
    }

    #[test]
    fn incomplete_entries_are_skipped_and_unknown_fields_ignored() {
        let markup = read(
            r#"{
            "highlights": [
                { "id": "no-color", "anchor": { "start": 0, "end": 1 }, "quoted_text": "x", "created": 1 },
                { "id": "ok", "anchor": { "start": 0, "end": 4 },
                  "quoted_text": "x", "color": 16777215, "created": 1, "future": true }
            ],
            "page_comments": [{ "id": "no-place", "created": 1, "text": "t" }]
        }"#,
        );
        assert_eq!(markup.highlights.len(), 1);
        assert_eq!(markup.highlights[0].id, "ok");
        assert!(markup.page_comments.is_empty());
        assert_eq!(read("7"), Test::default());
    }

    #[test]
    fn the_listed_markup_is_oldest_first_and_without_unwritten_page_comments() {
        let markup = with(
            vec![highlight("late", 9), highlight("early", 1)],
            vec![
                page_comment("b", 5, "x"),
                page_comment("blank", 2, ""),
                page_comment("a", 5, "y"),
            ],
        );
        let listed = markup.listed();
        let highlights: Vec<_> = listed.highlights.iter().map(|h| &h.id[..]).collect();
        let comments: Vec<_> = listed.page_comments.iter().map(|c| &c.id[..]).collect();
        assert_eq!(
            (highlights, comments),
            (vec!["early", "late"], vec!["a", "b"])
        );
    }

    #[test]
    fn a_color_change_replaces_the_color_and_nothing_else() {
        let mut markup = with(vec![highlight("a", 1), highlight("b", 2)], vec![]);
        let changes = &mut MarkupChanges::default();
        markup.set_highlight_color(changes, "a", RED, 5);
        markup.set_highlight_color(changes, "unknown", RED, 6);
        assert_eq!(markup.highlights[0].color, RED);
        assert_eq!(markup.highlights[1], highlight("b", 2));
        assert_eq!(changes.highlight_colors.len(), 1);
    }

    #[test]
    fn a_comment_is_set_replaced_and_deleted_by_emptying_it() {
        let mut markup = with(vec![highlight("a", 1)], vec![]);
        let changes = &mut MarkupChanges::default();
        markup.set_highlight_comment(changes, "a", "first", 1);
        markup.set_highlight_comment(changes, "a", "second", 2);
        assert_eq!(markup.highlights[0].comment.as_deref(), Some("second"));
        markup.set_highlight_comment(changes, "unknown", "x", 3);
        markup.set_highlight_comment(changes, "a", "", 4);
        assert_eq!(markup.highlights[0].comment, None);
        assert_eq!(changes.deleted_highlight_comments.get("a"), Some(&4));
    }

    #[test]
    fn removing_a_highlight_takes_its_comment_along_and_leaves_the_others() {
        let mut markup = with(vec![highlight("a", 1), highlight("b", 2)], vec![]);
        let changes = &mut MarkupChanges::default();
        markup.set_highlight_comment(changes, "a", "note", 1);
        markup.remove_highlight(changes, "a", 2);
        markup.remove_highlight(changes, "unknown", 3);
        assert_eq!(markup.highlights, vec![highlight("b", 2)]);
        assert!(changes.highlight_comments.is_empty());
        assert_eq!(changes.deleted_highlights.len(), 2);
    }

    #[test]
    fn a_page_comment_text_is_replaced_and_emptying_it_deletes_the_page_comment() {
        let mut markup = with(
            vec![],
            vec![page_comment("a", 1, ""), page_comment("b", 2, "keep")],
        );
        let changes = &mut MarkupChanges::default();
        markup.set_page_comment_text(changes, "a", "typed", 1);
        markup.set_page_comment_text(changes, "unknown", "x", 2);
        assert_eq!(markup.page_comments[0].text, "typed");
        markup.set_page_comment_text(changes, "a", "", 3);
        assert_eq!(markup.page_comments, vec![page_comment("b", 2, "keep")]);
        markup.remove_page_comment(changes, "b", 4);
        markup.remove_page_comment(changes, "unknown", 5);
        assert!(markup.page_comments.is_empty());
    }

    #[test]
    fn merging_unites_by_id_and_the_later_change_and_any_deletion_win() {
        let (mut left, mut right) = (
            with(vec![highlight("a", 1), highlight("gone", 2)], vec![]),
            with(vec![highlight("a", 1), highlight("b", 3)], vec![]),
        );
        let (mut left_changes, mut right_changes) = Default::default();
        left.set_highlight_comment(&mut left_changes, "a", "old", 10);
        right.set_highlight_comment(&mut right_changes, "a", "new", 20);
        right.set_highlight_color(&mut right_changes, "a", RED, 5);
        left.set_highlight_color(&mut left_changes, "a", YELLOW, 4);
        right.remove_highlight(&mut right_changes, "gone", 6);

        let merged = left.merged(&left_changes, &right, &right_changes);
        let again = right.merged(&right_changes, &left, &left_changes);
        assert_eq!(merged, again);
        let ids: Vec<_> = merged.highlights.iter().map(|h| &h.id[..]).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(merged.highlights[0].comment.as_deref(), Some("new"));
        assert_eq!(merged.highlights[0].color, RED);
    }

    #[test]
    fn changes_read_back_as_written_and_empty_ones_say_so() {
        let mut markup = with(vec![highlight("a", 1)], vec![]);
        let mut changes = MarkupChanges::default();
        assert!(changes.is_empty());
        markup.set_highlight_color(&mut changes, "a", RED, 9);
        assert!(!changes.is_empty());
        let json = object(changes.members());
        assert_eq!(MarkupChanges::from_json(&json), changes);
        assert_eq!(
            MarkupChanges::from_json(&Value::Null),
            MarkupChanges::default()
        );
    }
}
