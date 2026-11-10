//! A Book's state: what the app keeps about it next to its File, as JSON: its
//! Date Imported and its Markup (`markup`). Reading Progress and details have
//! files of their own (`progress`, `details`), as they change more often.

use crate::markup::Markup;
use pkm_base::item_id::ItemId;
use pkm_base::random::Entropy;
use serde_json::{Value, json};

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct BookState {
    /// Date Imported, in milliseconds since the Unix epoch.
    pub imported: u64,
    pub markup: Markup,
}

impl BookState {
    /// The state stored as `bytes`; none if they aren't one.
    pub(crate) fn decode(bytes: &[u8]) -> Option<Self> {
        let json: Value = serde_json::from_slice(bytes).ok()?;
        Some(Self {
            imported: json.get("imported")?.as_u64()?,
            markup: Markup::from_json(json.get("markup")?),
        })
    }

    pub(crate) fn encode(&self) -> Vec<u8> {
        json!({ "imported": self.imported, "markup": self.markup.to_json() })
            .to_string()
            .into_bytes()
    }
}

/// A new ID for a Highlight, a page Comment or a Custom Color, unique across
/// everything the app creates.
pub(crate) fn new_id() -> String {
    ItemId::new(&mut Entropy::from_system()).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::{Anchor, EpubPosition, Highlight, Rgb};

    #[test]
    fn a_state_reads_back_as_it_was_written() {
        let state = BookState {
            imported: 1_700_000_000_000,
            markup: Markup {
                highlights: vec![Highlight {
                    id: new_id(),
                    anchor: Anchor::Epub {
                        start: EpubPosition {
                            spine_index: 0,
                            offset: 1,
                        },
                        end: EpubPosition {
                            spine_index: 0,
                            offset: 5,
                        },
                    },
                    quoted_text: "text".into(),
                    color: Rgb { r: 1, g: 2, b: 3 },
                    created: 7,
                    comment: Some("c".into()),
                }],
                page_comments: vec![],
            },
        };
        assert_eq!(BookState::decode(&state.encode()), Some(state));
    }

    #[test]
    fn bytes_that_are_no_state_decode_to_none() {
        for bytes in [&b""[..], b"garbage", b"[]", br#"{"imported": 1}"#] {
            assert_eq!(BookState::decode(bytes), None);
        }
    }

    #[test]
    fn new_ids_differ() {
        assert_ne!(new_id(), new_id());
    }
}
