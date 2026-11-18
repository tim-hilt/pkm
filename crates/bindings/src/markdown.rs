//! The Markdown surface of the UniFFI library (ADR-0007). The logic lives in
//! `pkm_base::markdown`.

use pkm_base::markdown::{Inline, InlineSpan};

/// What an inline span is formatted as.
#[uniffi::remote(Enum)]
pub enum Inline {
    Emphasis,
    Strong,
    Code,
}

/// A formatted span at `start..end` (UTF-16 code units), whose first and last
/// `marker_len` units are its markers.
#[uniffi::remote(Record)]
pub struct InlineSpan {
    pub kind: Inline,
    pub start: u32,
    pub end: u32,
    pub marker_len: u32,
}

/// The Inline formatting of a Title, outer spans first.
#[uniffi::export]
pub fn parse_title(text: String) -> Vec<InlineSpan> {
    pkm_base::markdown::parse_title(&text)
}
