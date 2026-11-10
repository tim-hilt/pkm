//! A Book's state: what the app keeps about it besides its File, as one JSON
//! file in the Books folder's `.state` (see `store`): its Item ID, the File's
//! name and SHA-256, Date Imported, Markup (`markup`) and Reading Progress
//! (`progress`). The File itself is never changed.
//!
//! Besides what the user sees, the state carries when things changed:
//! `changes`, used only when two Devices' states merge (`sync`). It records
//! the last change of each field that can change, and what was deleted, so a
//! merge never brings back something the user removed.

use crate::json::{self, number, object, string};
use crate::markup::{Highlight, Markup, PageComment, Rgb};
use crate::progress::{self, ReadingProgress};
use pkm_base::item_id::ItemId;
use pkm_base::json::Value;
use pkm_base::random::Entropy;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BookState {
    /// The Book's Item ID, see `new_id`.
    pub id: String,
    /// The name of the File in the Books folder.
    pub file_name: String,
    /// SHA-256 of the File, lowercase hex, as of Import or the last change
    /// `Folder::scan` found.
    pub sha256: String,
    /// Date Imported, in milliseconds since the Unix epoch.
    pub imported: u64,
    pub markup: Markup,
    /// Where the user left off; none if the Book was never opened.
    pub progress: Option<ReadingProgress>,
    /// When changes were made, and what was deleted, for merging with another
    /// Device's state. Kept out of the public records: it is storage detail.
    pub(crate) changes: Changes,
}

/// When each part of a Book's state last changed, and what was deleted.
/// Times are milliseconds since the Unix epoch; an absent time means "before
/// this Device recorded times", which loses to any recorded one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    /// The File's name and SHA-256 changed together.
    file_at: u64,
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

impl Changes {
    fn is_empty(&self) -> bool {
        self.file_at == 0
            && self.highlight_colors.is_empty()
            && self.highlight_comments.is_empty()
            && self.deleted_highlight_comments.is_empty()
            && self.page_comment_texts.is_empty()
            && self.deleted_highlights.is_empty()
            && self.deleted_page_comments.is_empty()
    }

    fn merged(&self, other: &Changes) -> Changes {
        Changes {
            file_at: self.file_at.max(other.file_at),
            highlight_colors: latest_times(&self.highlight_colors, &other.highlight_colors),
            highlight_comments: latest_times(&self.highlight_comments, &other.highlight_comments),
            deleted_highlight_comments: latest_times(
                &self.deleted_highlight_comments,
                &other.deleted_highlight_comments,
            ),
            page_comment_texts: latest_times(&self.page_comment_texts, &other.page_comment_texts),
            deleted_highlights: latest_times(&self.deleted_highlights, &other.deleted_highlights),
            deleted_page_comments: latest_times(
                &self.deleted_page_comments,
                &other.deleted_page_comments,
            ),
        }
    }

    fn to_json(&self) -> Value {
        object([
            ("file_at", number(self.file_at)),
            ("highlight_colors", times_json(&self.highlight_colors)),
            ("highlight_comments", times_json(&self.highlight_comments)),
            (
                "deleted_highlight_comments",
                times_json(&self.deleted_highlight_comments),
            ),
            ("page_comment_texts", times_json(&self.page_comment_texts)),
            ("deleted_highlights", times_json(&self.deleted_highlights)),
            (
                "deleted_page_comments",
                times_json(&self.deleted_page_comments),
            ),
        ])
    }

    fn from_json(json: &Value) -> Changes {
        let times = |key: &str| read_times(json.get(key));
        Changes {
            file_at: json.get("file_at").and_then(Value::as_u64).unwrap_or(0),
            highlight_colors: times("highlight_colors"),
            highlight_comments: times("highlight_comments"),
            deleted_highlight_comments: times("deleted_highlight_comments"),
            page_comment_texts: times("page_comment_texts"),
            deleted_highlights: times("deleted_highlights"),
            deleted_page_comments: times("deleted_page_comments"),
        }
    }
}

fn latest_times(a: &BTreeMap<String, u64>, b: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    let mut latest = a.clone();
    for (id, at) in b {
        latest
            .entry(id.clone())
            .and_modify(|known| *known = (*known).max(*at))
            .or_insert(*at);
    }
    latest
}

fn times_json(times: &BTreeMap<String, u64>) -> Value {
    object(times.iter().map(|(id, at)| (id.as_str(), number(*at))))
}

fn read_times(json: Option<&Value>) -> BTreeMap<String, u64> {
    json.and_then(Value::as_object)
        .into_iter()
        .flat_map(|times| times.iter())
        .filter_map(|(id, at)| Some((id.to_owned(), at.as_u64()?)))
        .collect()
}

impl BookState {
    /// The state stored as `bytes`; none if they aren't one.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let json = json::parse(bytes)?;
        let text = |key| json.get(key)?.as_str().map(str::to_owned);
        Some(Self {
            id: ItemId::parse(json.get("id")?.as_str()?)?.to_string(),
            file_name: text("file_name")?,
            sha256: text("sha256")?,
            imported: json.get("imported")?.as_u64()?,
            markup: Markup::from_json(json.get("markup")?),
            progress: json
                .get("progress")
                .and_then(Value::as_str)
                .and_then(|line| progress::decode(line.as_bytes())),
            changes: json
                .get("changes")
                .map(Changes::from_json)
                .unwrap_or_default(),
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let progress = self.progress.as_ref().map_or(Value::Null, |p| {
            string(&String::from_utf8_lossy(&progress::encode(p)))
        });
        let mut members = vec![
            ("id", string(&self.id)),
            ("file_name", string(&self.file_name)),
            ("sha256", string(&self.sha256)),
            ("imported", number(self.imported)),
            ("markup", self.markup.to_json()),
            ("progress", progress),
        ];
        if !self.changes.is_empty() {
            members.push(("changes", self.changes.to_json()));
        }
        object(members).to_json().into_bytes()
    }

    /// Takes in what `other`, the state of the same File imported on another
    /// Device, holds: its Highlights and Comments this one lacks, and its
    /// Reading Progress if it was read later.
    pub(crate) fn absorb(&mut self, other: BookState) {
        let content = merge_content(self, &other);
        self.markup = content.markup;
        self.progress = content.progress;
        self.imported = self.imported.min(other.imported);
        self.changes = content.changes;
    }

    /// This state and `other`, the same Book's state on another Device, as
    /// one: Markup united by id with each change going to the later one and
    /// deletions kept, the newest Reading Progress, and the File description
    /// of the later change. Commutative and idempotent.
    pub(crate) fn merged(&self, other: &BookState) -> BookState {
        let (file_name, sha256, file_at) = if self.changes.file_at > other.changes.file_at {
            (
                self.file_name.clone(),
                self.sha256.clone(),
                self.changes.file_at,
            )
        } else if self.changes.file_at < other.changes.file_at {
            (
                other.file_name.clone(),
                other.sha256.clone(),
                other.changes.file_at,
            )
        } else if (&self.file_name, &self.sha256) <= (&other.file_name, &other.sha256) {
            (
                self.file_name.clone(),
                self.sha256.clone(),
                self.changes.file_at,
            )
        } else {
            (
                other.file_name.clone(),
                other.sha256.clone(),
                other.changes.file_at,
            )
        };
        let content = merge_content(self, other);
        BookState {
            id: self.id.clone(),
            file_name,
            sha256,
            imported: self.imported.min(other.imported),
            markup: content.markup,
            progress: content.progress,
            changes: Changes {
                file_at,
                ..content.changes
            },
        }
    }

    /// Notes that the File is now called `file_name` and has `sha256`, changed
    /// at `at`: the later of two Devices' File descriptions is the one merged.
    pub(crate) fn set_file(&mut self, file_name: &str, sha256: &str, at: u64) {
        self.file_name = file_name.to_owned();
        self.sha256 = sha256.to_owned();
        self.changes.file_at = at;
    }

    pub(crate) fn set_highlight_color(&mut self, id: &str, color: Rgb, at: u64) {
        self.markup.set_highlight_color(id, color);
        if self.markup.highlights.iter().any(|h| h.id == id) {
            self.changes.highlight_colors.insert(id.to_owned(), at);
        }
    }

    pub(crate) fn set_highlight_comment(&mut self, id: &str, text: &str, at: u64) {
        self.markup.set_highlight_comment(id, text);
        if self.markup.highlights.iter().any(|h| h.id == id) {
            self.changes.highlight_comments.insert(id.to_owned(), at);
            if text.is_empty() {
                self.changes
                    .deleted_highlight_comments
                    .insert(id.to_owned(), at);
            } else {
                self.changes.deleted_highlight_comments.remove(id);
            }
        }
    }

    pub(crate) fn remove_highlight(&mut self, id: &str, at: u64) {
        self.markup.remove_highlight(id);
        self.changes.highlight_colors.remove(id);
        self.changes.highlight_comments.remove(id);
        self.changes.deleted_highlight_comments.remove(id);
        self.changes.deleted_highlights.insert(id.to_owned(), at);
    }

    pub(crate) fn set_page_comment_text(&mut self, id: &str, text: &str, at: u64) {
        self.markup.set_page_comment_text(id, text);
        if self.markup.page_comments.iter().any(|c| c.id == id) {
            self.changes.page_comment_texts.insert(id.to_owned(), at);
        } else {
            self.changes.deleted_page_comments.insert(id.to_owned(), at);
        }
    }

    pub(crate) fn remove_page_comment(&mut self, id: &str, at: u64) {
        self.markup.remove_page_comment(id);
        self.changes.page_comment_texts.remove(id);
        self.changes.deleted_page_comments.insert(id.to_owned(), at);
    }
}

/// The Markup, Reading Progress and change times of two states, as one.
fn merge_content(a: &BookState, b: &BookState) -> Content {
    Content {
        markup: merge_markup(a, b),
        progress: latest_progress(a.progress, b.progress),
        changes: a.changes.merged(&b.changes),
    }
}

struct Content {
    markup: Markup,
    progress: Option<ReadingProgress>,
    pub(crate) changes: Changes,
}

/// The later of two values; a tie goes to the smaller value, so the result
/// doesn't depend on the order the two states are merged in.
fn later<T: Ord>(a: T, at_a: u64, b: T, at_b: u64) -> T {
    match at_a.cmp(&at_b) {
        std::cmp::Ordering::Greater => a,
        std::cmp::Ordering::Less => b,
        std::cmp::Ordering::Equal => a.min(b),
    }
}

fn time_of(times: &BTreeMap<String, u64>, id: &str) -> u64 {
    times.get(id).copied().unwrap_or(0)
}

/// A key that orders a Highlight's unchanging parts, for a deterministic tie.
fn highlight_key(h: &Highlight) -> String {
    format!("{:?}|{}|{}", h.anchor, h.quoted_text, h.created)
}

fn page_comment_key(c: &PageComment) -> String {
    format!("{:?}|{}", c.place, c.created)
}

fn merge_markup(a: &BookState, b: &BookState) -> Markup {
    let mut highlights: BTreeMap<&str, &Highlight> = BTreeMap::new();
    for h in a.markup.highlights.iter().chain(&b.markup.highlights) {
        highlights.insert(&h.id, h);
    }
    let merged_highlights = highlights
        .into_iter()
        .filter(|(id, _)| {
            !a.changes.deleted_highlights.contains_key(*id)
                && !b.changes.deleted_highlights.contains_key(*id)
        })
        .map(|(id, _)| {
            let left = a.markup.highlights.iter().find(|h| h.id == id);
            let right = b.markup.highlights.iter().find(|h| h.id == id);
            match (left, right) {
                (Some(left), Some(right)) => merge_highlight(left, a, right, b),
                (Some(one), None) | (None, Some(one)) => one.clone(),
                (None, None) => unreachable!("the id came from one of them"),
            }
        })
        .collect::<Vec<_>>();

    let mut comments: BTreeMap<&str, &PageComment> = BTreeMap::new();
    for c in a.markup.page_comments.iter().chain(&b.markup.page_comments) {
        comments.insert(&c.id, c);
    }
    let merged_comments = comments
        .into_iter()
        .filter(|(id, _)| {
            !a.changes.deleted_page_comments.contains_key(*id)
                && !b.changes.deleted_page_comments.contains_key(*id)
        })
        .map(|(id, _)| {
            let left = a.markup.page_comments.iter().find(|c| c.id == id);
            let right = b.markup.page_comments.iter().find(|c| c.id == id);
            match (left, right) {
                (Some(left), Some(right)) => merge_comment(left, a, right, b),
                (Some(one), None) | (None, Some(one)) => one.clone(),
                (None, None) => unreachable!("the id came from one of them"),
            }
        })
        .collect::<Vec<_>>();

    let mut markup = Markup {
        highlights: merged_highlights,
        page_comments: merged_comments,
    };
    markup
        .highlights
        .sort_by(|x, y| (x.created, &x.id).cmp(&(y.created, &y.id)));
    markup
        .page_comments
        .sort_by(|x, y| (x.created, &x.id).cmp(&(y.created, &y.id)));
    markup
}

fn merge_highlight(a: &Highlight, left: &BookState, b: &Highlight, right: &BookState) -> Highlight {
    let mut merged = if highlight_key(a) <= highlight_key(b) {
        a.clone()
    } else {
        b.clone()
    };
    let color = later(
        a.color.packed(),
        time_of(&left.changes.highlight_colors, &a.id),
        b.color.packed(),
        time_of(&right.changes.highlight_colors, &b.id),
    );
    merged.color = Rgb::unpacked(color).unwrap_or(merged.color);
    let comment = |h: &Highlight, state: &BookState| {
        let set = time_of(&state.changes.highlight_comments, &h.id);
        let deleted = time_of(&state.changes.deleted_highlight_comments, &h.id);
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

fn merge_comment(
    a: &PageComment,
    left: &BookState,
    b: &PageComment,
    right: &BookState,
) -> PageComment {
    let mut merged = if page_comment_key(a) <= page_comment_key(b) {
        a.clone()
    } else {
        b.clone()
    };
    merged.text = later(
        a.text.clone(),
        time_of(&left.changes.page_comment_texts, &a.id),
        b.text.clone(),
        time_of(&right.changes.page_comment_texts, &b.id),
    );
    merged
}

/// The Reading Progress that was last saved; a tie goes to the one encoded
/// first, so the result doesn't depend on the order.
fn latest_progress(
    a: Option<ReadingProgress>,
    b: Option<ReadingProgress>,
) -> Option<ReadingProgress> {
    match (a, b) {
        (Some(a), Some(b)) if a.last_read == b.last_read => {
            if progress::encode(&a) <= progress::encode(&b) {
                Some(a)
            } else {
                Some(b)
            }
        }
        (Some(a), Some(b)) => Some(if a.last_read > b.last_read { a } else { b }),
        (a, b) => a.or(b),
    }
}

/// A new Item ID: for a Book, and for a Highlight, a page Comment or a Custom
/// Color, unique across everything the app creates.
pub(crate) fn new_id() -> String {
    ItemId::new(&mut Entropy::from_system()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::{Anchor, EpubPosition, Highlight, Rgb};
    use crate::progress::Place;

    fn at(offset: u64) -> Anchor {
        Anchor::Epub {
            start: EpubPosition {
                spine_index: 0,
                offset,
            },
            end: EpubPosition {
                spine_index: 0,
                offset: offset + 1,
            },
        }
    }

    fn highlight(id: &str) -> Highlight {
        Highlight {
            id: id.into(),
            anchor: at(1),
            quoted_text: "text".into(),
            color: Rgb { r: 1, g: 2, b: 3 },
            created: 7,
            comment: Some("c".into()),
        }
    }

    fn state(id: &str, file_name: &str, sha256: &str) -> BookState {
        BookState {
            id: id.into(),
            file_name: file_name.into(),
            sha256: sha256.into(),
            imported: 1_700_000_000_000,
            markup: Markup::default(),
            progress: None,
            changes: Changes::default(),
        }
    }

    fn read_at(last_read: u64) -> Option<ReadingProgress> {
        Some(ReadingProgress {
            place: Place::Epub {
                spine_index: 1,
                offset: 2,
            },
            last_read,
        })
    }

    #[test]
    fn a_state_reads_back_as_it_was_written() {
        let mut full = state("abcdefghijkl", "Emma.epub", "00ff");
        full.markup.highlights.push(highlight("h"));
        full.progress = read_at(9);
        full.set_highlight_color("h", Rgb { r: 9, g: 9, b: 9 }, 99);
        full.set_highlight_comment("h", "changed", 100);
        full.remove_page_comment("gone", 101);
        assert_eq!(BookState::decode(&full.encode()), Some(full));
        let unread = state("abcdefghijkl", "Emma.epub", "00ff");
        assert_eq!(BookState::decode(&unread.encode()), Some(unread));
    }

    #[test]
    fn a_state_is_a_json_object_with_its_identity_and_progress() {
        let mut book = state("abcdefghijkl", "a.pdf", "ff");
        book.progress = read_at(9);
        let json = String::from_utf8(book.encode()).unwrap();
        assert!(
            json.starts_with(
                r#"{"file_name":"a.pdf","id":"abcdefghijkl","imported":1700000000000,"#
            )
        );
        assert!(json.ends_with(r#""progress":"e 1 2 9","sha256":"ff"}"#));
    }

    #[test]
    fn bytes_that_are_no_state_decode_to_none() {
        for bytes in [&b""[..], b"garbage", b"[]", br#"{"imported": 1}"#] {
            assert_eq!(BookState::decode(bytes), None);
        }
        // An id that is no Item ID.
        let mut book = state("abcdefghijkl", "a.pdf", "ff");
        book.id = "../x".into();
        assert_eq!(BookState::decode(&book.encode()), None);
    }

    #[test]
    fn absorbing_unites_markup_by_id_and_keeps_the_later_reading() {
        let mut kept = state("aaaaaaaaaaaa", "a.pdf", "ff");
        kept.markup.highlights.push(highlight("one"));
        kept.progress = read_at(5);
        let mut other = state("bbbbbbbbbbbb", "b.pdf", "ff");
        other.imported = 3;
        other.markup.highlights = vec![highlight("one"), highlight("two")];
        other.progress = read_at(8);
        kept.absorb(other);
        let ids: Vec<_> = kept.markup.highlights.iter().map(|h| &h.id[..]).collect();
        assert_eq!(ids, ["one", "two"]);
        assert_eq!(kept.progress, read_at(8));
        assert_eq!(kept.imported, 3);
    }

    #[test]
    fn new_ids_differ() {
        assert_ne!(new_id(), new_id());
    }

    #[test]
    fn a_deleted_highlight_never_comes_back_and_a_later_change_wins_its_field() {
        let mut left = state("abcdefghijkl", "a.pdf", "ff");
        left.markup.highlights.push(highlight("one"));
        left.set_highlight_color("one", Rgb { r: 1, g: 1, b: 1 }, 10);
        left.set_highlight_comment("one", "left", 20);
        let mut right = left.clone();
        right.set_highlight_color("one", Rgb { r: 2, g: 2, b: 2 }, 11);
        right.set_highlight_comment("one", "right", 19);

        let merged = left.merged(&right);
        assert_eq!(merged.markup.highlights[0].color, Rgb { r: 2, g: 2, b: 2 });
        assert_eq!(merged.markup.highlights[0].comment.as_deref(), Some("left"));
        assert_eq!(merged.merged(&left), merged.merged(&right));
        assert_eq!(merged.merged(&merged), merged);

        let mut removed = merged.clone();
        removed.remove_highlight("one", 30);
        assert!(removed.merged(&left).markup.highlights.is_empty());
        assert!(left.merged(&removed).markup.highlights.is_empty());
        assert!(removed.merged(&removed).markup.highlights.is_empty());
    }

    #[test]
    fn a_deleted_comment_stays_deleted_until_one_is_written_later() {
        let mut left = state("abcdefghijkl", "a.pdf", "ff");
        left.markup.highlights.push(highlight("one"));
        left.set_highlight_comment("one", "", 10);
        let mut right = state("abcdefghijkl", "a.pdf", "ff");
        right.markup.highlights.push(highlight("one"));
        right.set_highlight_comment("one", "written", 9);

        let merged = left.merged(&right);
        assert_eq!(merged.markup.highlights[0].comment, None);
        assert_eq!(merged.merged(&right), merged.merged(&left));
        assert_eq!(merged.merged(&merged), merged);

        let mut later = right.clone();
        later.set_highlight_comment("one", "again", 11);
        assert_eq!(
            merged.merged(&later).markup.highlights[0]
                .comment
                .as_deref(),
            Some("again")
        );
    }

    #[test]
    fn a_page_comment_is_removed_by_an_emptying() {
        let mut left = state("abcdefghijkl", "a.pdf", "ff");
        left.markup.page_comments.push(PageComment {
            id: "c".into(),
            place: Place::Epub {
                spine_index: 0,
                offset: 1,
            },
            created: 1,
            text: "kept".into(),
        });
        left.changes.page_comment_texts.insert("c".into(), 5);
        let mut right = left.clone();
        right.set_page_comment_text("c", "", 6);
        assert!(left.merged(&right).markup.page_comments.is_empty());
    }

    #[test]
    fn merging_states_is_order_independent_and_idempotent_over_generated_markup() {
        let mut seed = 0x5eed_1234_u64;
        let mut next = |bound: u64| {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) % bound
        };
        for _ in 0..200 {
            let mut left = state("abcdefghijkl", "a.pdf", "ff");
            let mut right = state("abcdefghijkl", "a.pdf", "ff");
            for side in [&mut left, &mut right] {
                side.imported = next(100);
                side.progress = (next(2) == 1).then(|| ReadingProgress {
                    place: Place::Epub {
                        spine_index: next(4) as u32,
                        offset: next(100),
                    },
                    last_read: next(100),
                });
                for i in 0..6 {
                    let id = format!("h{i}");
                    let present = next(2) == 1;
                    if present {
                        side.markup.highlights.push(Highlight {
                            id: id.clone(),
                            anchor: at(next(20)),
                            quoted_text: format!("text{i}"),
                            color: Rgb {
                                r: next(255) as u8,
                                g: next(255) as u8,
                                b: next(255) as u8,
                            },
                            created: next(20),
                            comment: (next(2) == 1).then(|| format!("comment{i}")),
                        });
                        if next(2) == 1 {
                            side.changes.highlight_colors.insert(id.clone(), next(100));
                        }
                        if next(2) == 1 {
                            side.changes
                                .highlight_comments
                                .insert(id.clone(), next(100));
                        }
                    }
                    if next(3) == 0 {
                        side.changes.deleted_highlights.insert(id, next(100));
                    }
                }
                for i in 0..4 {
                    let id = format!("c{i}");
                    if next(2) == 1 {
                        side.markup.page_comments.push(PageComment {
                            id: id.clone(),
                            place: Place::Epub {
                                spine_index: next(4) as u32,
                                offset: next(100),
                            },
                            created: next(20),
                            text: format!("text{i}"),
                        });
                        side.changes
                            .page_comment_texts
                            .insert(id.clone(), next(100));
                    }
                    if next(3) == 0 {
                        side.changes.deleted_page_comments.insert(id, next(100));
                    }
                }
            }
            let (a, b) = (left.encode(), right.encode());
            let ab = crate::sync::merge(None, &a, &b).expect("two states merge");
            let ba = crate::sync::merge(None, &b, &a).expect("two states merge");
            assert_eq!(ab, ba);
            let twice = crate::sync::merge(None, &ab, &ab).expect("a state merges with itself");
            assert_eq!(twice, ab);
        }
    }
}
