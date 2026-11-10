//! A Book's state: what the app keeps about it besides its File, as one JSON
//! file in the Books folder's `.state` (see `store`): its Item ID, the File's
//! name and SHA-256, Date Imported, Markup (`markup`) and Reading Progress
//! (`progress`). The File itself is never changed.

use crate::json::{self, number, object, string};
use crate::markup::Markup;
use crate::progress::{self, ReadingProgress};
use pkm_base::item_id::ItemId;
use pkm_base::json::Value;
use pkm_base::random::Entropy;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BookState {
    /// The Book's Item ID, see `new_id`.
    pub id: String,
    /// The name of the File in the Books folder.
    pub file_name: String,
    /// SHA-256 of the File, lowercase hex, as of Import.
    pub sha256: String,
    /// Date Imported, in milliseconds since the Unix epoch.
    pub imported: u64,
    pub markup: Markup,
    /// Where the user left off; none if the Book was never opened.
    pub progress: Option<ReadingProgress>,
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
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        let progress = self.progress.as_ref().map_or(Value::Null, |p| {
            string(&String::from_utf8_lossy(&progress::encode(p)))
        });
        object([
            ("id", string(&self.id)),
            ("file_name", string(&self.file_name)),
            ("sha256", string(&self.sha256)),
            ("imported", number(self.imported)),
            ("markup", self.markup.to_json()),
            ("progress", progress),
        ])
        .to_json()
        .into_bytes()
    }

    /// Takes in what `other`, the state of the same File imported on another
    /// Device, holds: its Highlights and Comments this one lacks, and its
    /// Reading Progress if it was read later.
    pub(crate) fn absorb(&mut self, other: BookState) {
        let Markup {
            highlights,
            page_comments,
        } = other.markup;
        for highlight in highlights {
            if !self.markup.highlights.iter().any(|h| h.id == highlight.id) {
                self.markup.highlights.push(highlight);
            }
        }
        for comment in page_comments {
            if !self.markup.page_comments.iter().any(|c| c.id == comment.id) {
                self.markup.page_comments.push(comment);
            }
        }
        let last_read = |p: &Option<ReadingProgress>| p.map(|p| p.last_read);
        if last_read(&other.progress) > last_read(&self.progress) {
            self.progress = other.progress;
        }
        self.imported = self.imported.min(other.imported);
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

    fn highlight(id: &str) -> Highlight {
        let at = EpubPosition {
            spine_index: 0,
            offset: 1,
        };
        Highlight {
            id: id.into(),
            anchor: Anchor::Epub { start: at, end: at },
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
}
