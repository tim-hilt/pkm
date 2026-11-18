//! The Markdown surface of the UniFFI library (ADR-0007). The logic lives in
//! `pkm_base::markdown`.

use pkm_base::markdown::{Block, BlockSpan, Description, Inline, InlineSpan, LinkSpan, TextRange};

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

/// What a block of a Description is.
#[uniffi::remote(Enum)]
pub enum Block {
    Paragraph,
    Heading,
    Bullet,
    Numbered,
    Quote,
    CodeFence,
    Image,
}

/// A block at `start..end` with its marker at `marker_start..marker_end`.
#[uniffi::remote(Record)]
pub struct BlockSpan {
    pub kind: Block,
    pub start: u32,
    pub end: u32,
    pub marker_start: u32,
    pub marker_end: u32,
    pub level: u32,
    pub number: u32,
    pub target: String,
}

/// A link `[text](target)`; its markers are `start..text_start` and `text_end..end`.
#[uniffi::remote(Record)]
pub struct LinkSpan {
    pub start: u32,
    pub end: u32,
    pub text_start: u32,
    pub text_end: u32,
    pub target: String,
}

/// A parsed Description.
#[uniffi::remote(Record)]
pub struct Description {
    pub blocks: Vec<BlockSpan>,
    pub inlines: Vec<InlineSpan>,
    pub links: Vec<LinkSpan>,
}

/// A range in UTF-16 code units.
#[uniffi::remote(Record)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

/// The blocks, Inline formatting and links of a Description.
#[uniffi::export]
pub fn parse_description(text: String) -> Description {
    pkm_base::markdown::parse_description(&text)
}

/// What a caret (UTF-16 offset) shows raw: its line, or the whole fence it is in.
#[uniffi::export]
pub fn raw_range(text: String, caret: u32) -> TextRange {
    pkm_base::markdown::raw_range(&text, caret)
}
