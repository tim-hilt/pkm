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
    /// The state stored as `text`; the default if it is none. The first
    /// version kept only `<offset> <last_read>`.
    pub(crate) fn decode(text: &str) -> Self {
        let Ok(json) = json::parse(text) else {
            return Self::decode_legacy(text);
        };
        let number = |key| json.get("progress")?.get(key)?.as_u64();
        Self {
            progress: number("offset").zip(number("last_read")),
            markup: json
                .get("markup")
                .map(Markup::from_json)
                .unwrap_or_default(),
            changes: json
                .get("changes")
                .map(MarkupChanges::from_json)
                .unwrap_or_default(),
        }
    }

    fn decode_legacy(line: &str) -> Self {
        let progress = line
            .split_once(' ')
            .and_then(|(offset, last_read)| Some((offset.parse().ok()?, last_read.parse().ok()?)));
        Self {
            progress,
            ..Self::default()
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
    fn the_first_versions_line_is_still_read() {
        let state = State::decode("12 3456");
        assert_eq!(state.progress, Some((12, 3456)));
        assert!(state.markup.highlights.is_empty());
    }

    #[test]
    fn junk_is_no_state() {
        assert_eq!(State::decode("junk"), State::default());
        assert_eq!(State::decode(""), State::default());
    }
}
