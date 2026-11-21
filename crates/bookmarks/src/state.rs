//! What the app keeps about a Bookmark's Article besides the Article itself,
//! in the Bookmark's hidden state file (Bookmarks ADR-0002): Reading Progress
//! and Markup, anchored by UTF-16 offsets into the Article's stored Markdown,
//! with the quoted text as fallback. Markup is the model Books share
//! (`pkm_base::markup`, ADR-0034); `changes` records when it changed, for
//! merging by Sync.

use pkm_base::json::{self, Value, number, object};
use pkm_base::markup::{self, Located, MarkupChanges};

/// A passage of the Article: from UTF-16 offset `start` up to, not including, `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Anchor {
    pub start: u64,
    pub end: u64,
}

/// A spot in the Article: a UTF-16 offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    pub offset: u64,
}

pub type Highlight = markup::Highlight<Anchor>;
pub type PageComment = markup::PageComment<Place>;
pub type Markup = markup::Markup<Anchor, Place>;
pub use pkm_base::markup::Rgb;

impl Located for Anchor {
    fn to_json(&self) -> Value {
        object([("start", number(self.start)), ("end", number(self.end))])
    }

    fn from_json(json: &Value) -> Option<Self> {
        Some(Self {
            start: json.get("start")?.as_u64()?,
            end: json.get("end")?.as_u64()?,
        })
    }
}

impl Located for Place {
    fn to_json(&self) -> Value {
        object([("offset", number(self.offset))])
    }

    fn from_json(json: &Value) -> Option<Self> {
        Some(Self {
            offset: json.get("offset")?.as_u64()?,
        })
    }
}

/// The state of one Bookmark. Nothing in it is the default.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct State {
    /// Where the Article was left, and when (Last Read, milliseconds since the Unix epoch).
    pub progress: Option<(u64, u64)>,
    pub markup: Markup,
    pub changes: MarkupChanges,
}

impl State {
    /// The state stored as `text`; the default if it is none.
    pub(crate) fn decode(text: &str) -> Self {
        Self::parse(text).unwrap_or_default()
    }

    /// The state stored as `text`; none if `text` isn't one.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let json = json::parse(text).ok()?;
        let markup = json.get("markup")?;
        let number = |key| json.get("progress")?.get(key)?.as_u64();
        Some(Self {
            progress: number("offset").zip(number("last_read")),
            markup: Markup::from_json(markup),
            changes: json
                .get("changes")
                .map(MarkupChanges::from_json)
                .unwrap_or_default(),
        })
    }

    /// This state and `other`, the same Bookmark's on another Device, as one:
    /// Markup united by id with each change going to the later one and
    /// deletions kept, and the latest Reading Progress. Commutative and
    /// idempotent.
    pub(crate) fn merged(&self, other: &Self) -> Self {
        Self {
            // The later reading; a tie goes to the larger offset, so the
            // result doesn't depend on the order.
            progress: match (self.progress, other.progress) {
                (Some(a), Some(b)) => Some(if (a.1, a.0) >= (b.1, b.0) { a } else { b }),
                (a, b) => a.or(b),
            },
            markup: self
                .markup
                .merged(&self.changes, &other.markup, &other.changes),
            changes: self.changes.merged(&other.changes),
        }
    }

    pub(crate) fn encode(&self) -> String {
        let mut members = vec![("markup", self.markup.to_json())];
        if let Some((offset, last_read)) = self.progress {
            let progress = object([("offset", number(offset)), ("last_read", number(last_read))]);
            members.push(("progress", progress));
        }
        if !self.changes.is_empty() {
            members.push(("changes", object(self.changes.members())));
        }
        object(members).to_json()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_state_survives_encoding() {
        let mut state = State {
            progress: Some((7, 99)),
            ..State::default()
        };
        state.markup.highlights.push(Highlight {
            id: "h".into(),
            anchor: Anchor { start: 1, end: 4 },
            quoted_text: "abc".into(),
            color: Rgb { r: 1, g: 2, b: 3 },
            created: 5,
            comment: Some("why".into()),
        });
        state.markup.page_comments.push(PageComment {
            id: "c".into(),
            place: Place { offset: 2 },
            created: 6,
            text: "hm".into(),
        });
        state.markup.remove_highlight(&mut state.changes, "gone", 8);
        assert_eq!(State::decode(&state.encode()), state);
    }

    #[test]
    fn junk_is_no_state() {
        assert_eq!(State::decode("junk"), State::default());
        assert_eq!(State::decode(""), State::default());
        for junk in ["junk", "", "{}", "12 3456"] {
            assert_eq!(State::parse(junk), None, "{junk:?}");
        }
    }

    #[test]
    fn states_merge_to_the_later_reading_and_all_markup() {
        let highlight = |id: &str| Highlight {
            id: id.into(),
            anchor: Anchor { start: 1, end: 2 },
            quoted_text: "a".into(),
            color: Rgb { r: 1, g: 2, b: 3 },
            created: 5,
            comment: None,
        };
        let mut left = State {
            progress: Some((10, 100)),
            ..State::default()
        };
        left.markup.highlights.push(highlight("l"));
        let mut right = State {
            progress: Some((3, 200)),
            ..State::default()
        };
        right.markup.highlights.push(highlight("r"));
        let merged = left.merged(&right);
        assert_eq!(merged.progress, Some((3, 200)));
        assert_eq!(merged.markup.highlights.len(), 2);
        assert_eq!(merged, right.merged(&left));
        assert_eq!(merged.merged(&merged), merged);
    }
}
