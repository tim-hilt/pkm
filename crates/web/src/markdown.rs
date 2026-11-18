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
#[derive(Clone)]
pub struct InlineSpan {
    pub kind: Inline,
    pub start: u32,
    pub end: u32,
    #[wasm_bindgen(js_name = markerLen)]
    pub marker_len: u32,
}

impl From<pkm_base::markdown::InlineSpan> for InlineSpan {
    fn from(s: pkm_base::markdown::InlineSpan) -> Self {
        Self {
            kind: s.kind.into(),
            start: s.start,
            end: s.end,
            marker_len: s.marker_len,
        }
    }
}

/// The Inline formatting of a Title, outer spans first.
#[wasm_bindgen(js_name = parseTitle)]
pub fn parse_title(text: &str) -> Vec<InlineSpan> {
    pkm_base::markdown::parse_title(text)
        .into_iter()
        .map(Into::into)
        .collect()
}

mirror_enum!(
    /// What a block of a Description is.
    Block in pkm_base::markdown::Block {
        Paragraph, Heading, Bullet, Numbered, Quote, CodeFence, Image
    }
);

/// A block at `start..end` with its marker at `markerStart..markerEnd`; a
/// fence's closing marker is `closingStart..closingEnd`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct BlockSpan {
    pub kind: Block,
    pub start: u32,
    pub end: u32,
    #[wasm_bindgen(js_name = markerStart)]
    pub marker_start: u32,
    #[wasm_bindgen(js_name = markerEnd)]
    pub marker_end: u32,
    pub level: u32,
    pub number: u32,
    /// An Image's reference.
    #[wasm_bindgen(getter_with_clone)]
    pub target: String,
    #[wasm_bindgen(js_name = closingStart)]
    pub closing_start: u32,
    #[wasm_bindgen(js_name = closingEnd)]
    pub closing_end: u32,
}

/// A link `[text](target)`; its markers are `start..textStart` and `textEnd..end`.
#[wasm_bindgen]
#[derive(Clone)]
pub struct LinkSpan {
    pub start: u32,
    pub end: u32,
    #[wasm_bindgen(js_name = textStart)]
    pub text_start: u32,
    #[wasm_bindgen(js_name = textEnd)]
    pub text_end: u32,
    #[wasm_bindgen(getter_with_clone)]
    pub target: String,
}

/// A parsed Description.
#[wasm_bindgen(getter_with_clone)]
pub struct Description {
    pub blocks: Vec<BlockSpan>,
    pub inlines: Vec<InlineSpan>,
    pub links: Vec<LinkSpan>,
}

/// The blocks, Inline formatting and links of a Description.
#[wasm_bindgen(js_name = parseDescription)]
pub fn parse_description(text: &str) -> Description {
    let d = pkm_base::markdown::parse_description(text);
    Description {
        blocks: d
            .blocks
            .into_iter()
            .map(|b| BlockSpan {
                kind: b.kind.into(),
                start: b.start,
                end: b.end,
                marker_start: b.marker_start,
                marker_end: b.marker_end,
                level: b.level,
                number: b.number,
                target: b.target,
                closing_start: b.closing_start,
                closing_end: b.closing_end,
            })
            .collect(),
        inlines: d.inlines.into_iter().map(Into::into).collect(),
        links: d
            .links
            .into_iter()
            .map(|l| LinkSpan {
                start: l.start,
                end: l.end,
                text_start: l.text_start,
                text_end: l.text_end,
                target: l.target,
            })
            .collect(),
    }
}

/// A range in UTF-16 code units.
#[wasm_bindgen]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// What a caret (UTF-16 offset) shows raw: its line, or the whole fence it is in.
#[wasm_bindgen(js_name = rawRange)]
pub fn raw_range(text: &str, caret: u32) -> TextRange {
    let r = pkm_base::markdown::raw_range(text, caret);
    TextRange {
        start: r.start,
        end: r.end,
    }
}

/// A change to a text: `start..end` (UTF-16 code units) is replaced by
/// `replacement`, and the selection is `selectionStart..selectionEnd` of the
/// text after the change.
#[wasm_bindgen(getter_with_clone)]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub replacement: String,
    #[wasm_bindgen(js_name = selectionStart)]
    pub selection_start: u32,
    #[wasm_bindgen(js_name = selectionEnd)]
    pub selection_end: u32,
}

impl From<pkm_base::markdown::TextEdit> for TextEdit {
    fn from(e: pkm_base::markdown::TextEdit) -> Self {
        Self {
            start: e.start,
            end: e.end,
            replacement: e.replacement,
            selection_start: e.selection_start,
            selection_end: e.selection_end,
        }
    }
}

/// The edit for Return with the selection `start..end`: continues or ends a
/// list item, else a plain line break.
#[wasm_bindgen(js_name = returnAt)]
pub fn return_at(text: &str, start: u32, end: u32) -> TextEdit {
    pkm_base::markdown::return_at(text, start, end).into()
}

/// The edit that indents or outdents the list items touched by the selection
/// by two spaces; `undefined` when nothing changes.
#[wasm_bindgen(js_name = shiftItems)]
pub fn shift_items(text: &str, start: u32, end: u32, outdent: bool) -> Option<TextEdit> {
    pkm_base::markdown::shift_items(text, start, end, outdent).map(Into::into)
}

/// The edit for ⌘B, ⌘I or ⌘E with the selection `start..end`: removes the
/// `kind` markers around or in the selection, else wraps it in them.
#[wasm_bindgen(js_name = toggleInline)]
pub fn toggle_inline(text: &str, start: u32, end: u32, kind: Inline) -> TextEdit {
    pkm_base::markdown::toggle_inline(text, start, end, kind.into()).into()
}

/// The line a Task row shows for a Description; format it like a Title.
#[wasm_bindgen(js_name = rowPreview)]
pub fn row_preview(text: &str) -> String {
    pkm_base::markdown::row_preview(text)
}
