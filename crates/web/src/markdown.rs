//! The Markdown surface of the wasm exports, mirroring
//! `crates/bindings/src/markdown.rs`.

use wasm_bindgen::prelude::*;

mirror_enum!(
    /// What an inline span is formatted as.
    Inline in pkm_base::markdown::Inline { Emphasis, Strong, Code }
);

/// A formatted span at `start..end` (UTF-16 code units), whose first and last
/// `markerLen` units are its markers.
#[wasm_bindgen]
pub struct InlineSpan {
    pub kind: Inline,
    pub start: u32,
    pub end: u32,
    #[wasm_bindgen(js_name = markerLen)]
    pub marker_len: u32,
}

/// The Inline formatting of a Title, outer spans first.
#[wasm_bindgen(js_name = parseTitle)]
pub fn parse_title(text: &str) -> Vec<InlineSpan> {
    pkm_base::markdown::parse_title(text)
        .into_iter()
        .map(|s| InlineSpan {
            kind: s.kind.into(),
            start: s.start,
            end: s.end,
            marker_len: s.marker_len,
        })
        .collect()
}
