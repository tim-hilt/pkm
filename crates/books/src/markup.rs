//! Markup: Highlights and Comments on a Book, and how they are stored as JSON
//! in the Book's state (`state`).
//!
//! The anchor of a PDF Highlight is a list of pages with their rectangles in PDF
//! page space (points, origin at the page's bottom-left), engine-neutral
//! (ADR-0001). That of an EPUB Highlight is a start and an end position in the
//! text (ADR-0006). The quoted text is the fallback of both.
//!
//! A Comment on a Highlight is empty never: emptying it deletes it. A page
//! Comment is its text: emptying it deletes the page Comment, and one without
//! text, which only exists between adding it and typing, isn't listed.

use crate::progress::Place;
use serde_json::{Value, json};

/// A rectangle in PDF page space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The rectangles a Highlight covers on one page: usually one per line.
#[derive(Debug, Clone, PartialEq)]
pub struct PageRects {
    /// Zero-based index of the page.
    pub page_index: u32,
    pub rects: Vec<Rect>,
}

/// A character of an EPUB's text (ADR-0006).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EpubPosition {
    /// Zero-based index of the spine item.
    pub spine_index: u32,
    /// Offset into that item's text, in UTF-16 code units.
    pub offset: u64,
}

/// Where a Highlight is, in the terms of its Book's Format.
#[derive(Debug, Clone, PartialEq)]
pub enum Anchor {
    /// The rectangles it covers on each page, in page order.
    Pdf(Vec<PageRects>),
    /// From `start` up to, not including, `end`. They may be in different
    /// spine items.
    Epub {
        start: EpubPosition,
        end: EpubPosition,
    },
}

/// A color in sRGB, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub(crate) fn packed(self) -> u64 {
        u64::from(self.r) << 16 | u64::from(self.g) << 8 | u64::from(self.b)
    }

    pub(crate) fn unpacked(value: u64) -> Option<Self> {
        let [_, _, _, _, _, r, g, b] = value.to_be_bytes();
        (value <= 0xFF_FFFF).then_some(Self { r, g, b })
    }
}

/// A passage of a Book's text marked in a color. It may run across pages.
#[derive(Debug, Clone, PartialEq)]
pub struct Highlight {
    /// Unique among all Highlights.
    pub id: String,
    pub anchor: Anchor,
    /// The whole passage, however many pages it spans. For an EPUB it is the
    /// text from `start` to `end`, the spine items' texts joined by `\n`.
    pub quoted_text: String,
    pub color: Rgb,
    /// When it was added, in milliseconds since the Unix epoch.
    pub created: u64,
    /// What the user wrote about the passage. Never empty.
    pub comment: Option<String>,
}

/// A Comment on a spot of a page, belonging to no passage.
#[derive(Debug, Clone, PartialEq)]
pub struct PageComment {
    /// Unique among all page Comments.
    pub id: String,
    /// The spot, in the terms of its Book's Format.
    pub place: Place,
    /// When it was added, in milliseconds since the Unix epoch.
    pub created: u64,
    /// What the user wrote. Never empty in the Markup.
    pub text: String,
}

/// Everything the user added on top of a Book.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Markup {
    /// Oldest first.
    pub highlights: Vec<Highlight>,
    /// Oldest first.
    pub page_comments: Vec<PageComment>,
}

impl Markup {
    /// The Markup as the Reader lists it: oldest first, without the page
    /// Comments that have no text yet.
    pub(crate) fn listed(&self) -> Markup {
        let mut markup = Markup {
            highlights: self.highlights.clone(),
            page_comments: self
                .page_comments
                .iter()
                .filter(|c| !c.text.is_empty())
                .cloned()
                .collect(),
        };
        markup
            .highlights
            .sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
        markup
            .page_comments
            .sort_by(|a, b| (a.created, &a.id).cmp(&(b.created, &b.id)));
        markup
    }

    /// Sets the color of Highlight `id`; nothing if there is none.
    pub(crate) fn set_highlight_color(&mut self, id: &str, color: Rgb) {
        if let Some(h) = self.highlights.iter_mut().find(|h| h.id == id) {
            h.color = color;
        }
    }

    /// Sets the Comment of Highlight `id`; empty `text` deletes it.
    pub(crate) fn set_highlight_comment(&mut self, id: &str, text: &str) {
        if let Some(h) = self.highlights.iter_mut().find(|h| h.id == id) {
            h.comment = (!text.is_empty()).then(|| text.to_owned());
        }
    }

    pub(crate) fn remove_highlight(&mut self, id: &str) {
        self.highlights.retain(|h| h.id != id);
    }

    /// Sets the text of page Comment `id`; empty `text` deletes it.
    pub(crate) fn set_page_comment_text(&mut self, id: &str, text: &str) {
        if text.is_empty() {
            self.remove_page_comment(id);
        } else if let Some(c) = self.page_comments.iter_mut().find(|c| c.id == id) {
            c.text = text.to_owned();
        }
    }

    pub(crate) fn remove_page_comment(&mut self, id: &str) {
        self.page_comments.retain(|c| c.id != id);
    }

    /// The Markup as JSON, as `from_json` reads it.
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "highlights": self.highlights.iter().map(highlight_json).collect::<Vec<_>>(),
            "page_comments": self.page_comments.iter().map(page_comment_json).collect::<Vec<_>>(),
        })
    }

    /// The Markup in `json`; entries that are incomplete are skipped.
    pub(crate) fn from_json(json: &Value) -> Markup {
        let list = |key: &str| {
            json.get(key)
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
        };
        Markup {
            highlights: list("highlights").filter_map(read_highlight).collect(),
            page_comments: list("page_comments")
                .filter_map(read_page_comment)
                .collect(),
        }
    }
}

fn highlight_json(h: &Highlight) -> Value {
    let anchor = match &h.anchor {
        Anchor::Pdf(pages) => json!({ "pdf": pages.iter().map(|p| json!({
            "page_index": p.page_index,
            "rects": p.rects.iter().flat_map(|r| [r.x, r.y, r.width, r.height]).collect::<Vec<_>>(),
        })).collect::<Vec<_>>() }),
        Anchor::Epub { start, end } => json!({ "epub": {
            "start_spine": start.spine_index, "start_offset": start.offset,
            "end_spine": end.spine_index, "end_offset": end.offset,
        }}),
    };
    let mut json = json!({
        "id": h.id, "anchor": anchor, "quoted_text": h.quoted_text,
        "color": h.color.packed(), "created": h.created,
    });
    if let Some(comment) = &h.comment {
        json["comment"] = comment.as_str().into();
    }
    json
}

fn page_comment_json(c: &PageComment) -> Value {
    let place = match c.place {
        Place::Pdf { page_index, x, y } => {
            json!({ "pdf": { "page_index": page_index, "x": x, "y": y } })
        }
        Place::Epub {
            spine_index,
            offset,
        } => json!({ "epub": { "spine": spine_index, "offset": offset } }),
    };
    json!({ "id": c.id, "place": place, "created": c.created, "text": c.text })
}

fn u32_at(json: &Value, key: &str) -> Option<u32> {
    u32::try_from(json.get(key)?.as_u64()?).ok()
}

fn read_highlight(json: &Value) -> Option<Highlight> {
    let anchor = json.get("anchor")?;
    let anchor = if let Some(pages) = anchor.get("pdf") {
        Anchor::Pdf(
            pages
                .as_array()?
                .iter()
                .map(read_page_rects)
                .collect::<Option<_>>()?,
        )
    } else {
        let epub = anchor.get("epub")?;
        let position = |spine: &str, offset: &str| {
            Some(EpubPosition {
                spine_index: u32_at(epub, spine)?,
                offset: epub.get(offset)?.as_u64()?,
            })
        };
        Anchor::Epub {
            start: position("start_spine", "start_offset")?,
            end: position("end_spine", "end_offset")?,
        }
    };
    Some(Highlight {
        id: json.get("id")?.as_str()?.to_owned(),
        anchor,
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

fn read_page_rects(json: &Value) -> Option<PageRects> {
    let numbers: Vec<f64> = json
        .get("rects")?
        .as_array()?
        .iter()
        .map(Value::as_f64)
        .collect::<Option<_>>()?;
    Some(PageRects {
        page_index: u32_at(json, "page_index")?,
        rects: numbers
            .as_chunks::<4>()
            .0
            .iter()
            .map(|&[x, y, width, height]| Rect {
                x,
                y,
                width,
                height,
            })
            .collect(),
    })
}

fn read_page_comment(json: &Value) -> Option<PageComment> {
    let place = json.get("place")?;
    let place = if let Some(epub) = place.get("epub") {
        Place::Epub {
            spine_index: u32_at(epub, "spine")?,
            offset: epub.get("offset")?.as_u64()?,
        }
    } else {
        let pdf = place.get("pdf")?;
        Place::Pdf {
            page_index: u32_at(pdf, "page_index")?,
            x: pdf.get("x")?.as_f64()?,
            y: pdf.get("y")?.as_f64()?,
        }
    };
    Some(PageComment {
        id: json.get("id")?.as_str()?.to_owned(),
        place,
        created: json.get("created")?.as_u64()?,
        text: json.get("text")?.as_str()?.to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pdf_highlight(id: &str, created: u64) -> Highlight {
        Highlight {
            id: id.into(),
            anchor: Anchor::Pdf(vec![PageRects {
                page_index: 2,
                rects: vec![Rect {
                    x: 1.5,
                    y: 2.0,
                    width: 30.25,
                    height: 4.0,
                }],
            }]),
            quoted_text: "quoted — “text”".into(),
            color: Rgb {
                r: 255,
                g: 235,
                b: 60,
            },
            created,
            comment: None,
        }
    }

    fn epub_highlight(id: &str, created: u64) -> Highlight {
        Highlight {
            anchor: Anchor::Epub {
                start: EpubPosition {
                    spine_index: 1,
                    offset: 10,
                },
                end: EpubPosition {
                    spine_index: 2,
                    offset: 3,
                },
            },
            ..pdf_highlight(id, created)
        }
    }

    fn page_comment(id: &str, created: u64, text: &str) -> PageComment {
        PageComment {
            id: id.into(),
            place: Place::Pdf {
                page_index: 4,
                x: 10.5,
                y: 20.0,
            },
            created,
            text: text.into(),
        }
    }

    fn round_trip(markup: &Markup) -> Markup {
        Markup::from_json(&serde_json::from_str(&markup.to_json().to_string()).unwrap())
    }

    #[test]
    fn markup_reads_back_as_it_was_written() {
        let markup = Markup {
            highlights: vec![
                Highlight {
                    comment: Some("why\nnot".into()),
                    ..pdf_highlight("a", 1)
                },
                epub_highlight("b", 2),
            ],
            page_comments: vec![
                page_comment("c", 3, "spot"),
                PageComment {
                    place: Place::Epub {
                        spine_index: 3,
                        offset: 99,
                    },
                    ..page_comment("d", 4, "epub spot")
                },
            ],
        };
        assert_eq!(round_trip(&markup), markup);
    }

    #[test]
    fn colors_keep_all_their_channels() {
        let mut h = pdf_highlight("a", 1);
        h.color = Rgb { r: 1, g: 2, b: 255 };
        let markup = Markup {
            highlights: vec![h],
            page_comments: vec![],
        };
        assert_eq!(round_trip(&markup), markup);
    }

    #[test]
    fn incomplete_entries_are_skipped_and_unknown_fields_ignored() {
        let json = json!({
            "highlights": [
                { "id": "no-color", "anchor": { "epub": {} }, "quoted_text": "x", "created": 1 },
                { "id": "ok", "anchor": { "epub": {
                    "start_spine": 0, "start_offset": 0, "end_spine": 0, "end_offset": 4 } },
                  "quoted_text": "x", "color": 16777215, "created": 1, "future": true },
            ],
            "page_comments": [{ "id": "no-place", "created": 1, "text": "t" }],
        });
        let markup = Markup::from_json(&json);
        assert_eq!(markup.highlights.len(), 1);
        assert_eq!(markup.highlights[0].id, "ok");
        assert!(markup.page_comments.is_empty());
        assert_eq!(Markup::from_json(&json!(7)), Markup::default());
    }

    #[test]
    fn the_listed_markup_is_oldest_first_and_without_unwritten_page_comments() {
        let markup = Markup {
            highlights: vec![pdf_highlight("late", 9), pdf_highlight("early", 1)],
            page_comments: vec![
                page_comment("b", 5, "x"),
                page_comment("blank", 2, ""),
                page_comment("a", 5, "y"),
            ],
        };
        let listed = markup.listed();
        let ids = |h: &Markup| {
            (
                h.highlights
                    .iter()
                    .map(|h| h.id.clone())
                    .collect::<Vec<_>>(),
                h.page_comments
                    .iter()
                    .map(|c| c.id.clone())
                    .collect::<Vec<_>>(),
            )
        };
        assert_eq!(
            ids(&listed),
            (
                vec!["early".into(), "late".into()],
                vec!["a".into(), "b".into()]
            )
        );
    }

    #[test]
    fn a_color_change_replaces_the_color_and_nothing_else() {
        let mut markup = Markup {
            highlights: vec![pdf_highlight("a", 1), pdf_highlight("b", 2)],
            page_comments: vec![],
        };
        let red = Rgb { r: 255, g: 0, b: 0 };
        markup.set_highlight_color("a", red);
        markup.set_highlight_color("unknown", red);
        assert_eq!(markup.highlights[0].color, red);
        assert_eq!(markup.highlights[1], pdf_highlight("b", 2));
    }

    #[test]
    fn a_comment_is_set_replaced_and_deleted_by_emptying_it() {
        let mut markup = Markup {
            highlights: vec![pdf_highlight("a", 1)],
            page_comments: vec![],
        };
        markup.set_highlight_comment("a", "first");
        markup.set_highlight_comment("a", "second");
        assert_eq!(markup.highlights[0].comment.as_deref(), Some("second"));
        markup.set_highlight_comment("unknown", "x");
        markup.set_highlight_comment("a", "");
        assert_eq!(markup.highlights[0].comment, None);
    }

    #[test]
    fn removing_a_highlight_takes_its_comment_along_and_leaves_the_others() {
        let mut markup = Markup {
            highlights: vec![pdf_highlight("a", 1), pdf_highlight("b", 2)],
            page_comments: vec![],
        };
        markup.set_highlight_comment("a", "note");
        markup.remove_highlight("a");
        markup.remove_highlight("unknown");
        assert_eq!(markup.highlights, vec![pdf_highlight("b", 2)]);
    }

    #[test]
    fn a_page_comment_text_is_replaced_and_emptying_it_deletes_the_page_comment() {
        let mut markup = Markup {
            highlights: vec![],
            page_comments: vec![page_comment("a", 1, ""), page_comment("b", 2, "keep")],
        };
        markup.set_page_comment_text("a", "typed");
        markup.set_page_comment_text("unknown", "x");
        assert_eq!(markup.page_comments[0].text, "typed");
        markup.set_page_comment_text("a", "");
        assert_eq!(markup.page_comments, vec![page_comment("b", 2, "keep")]);
        markup.remove_page_comment("b");
        markup.remove_page_comment("unknown");
        assert!(markup.page_comments.is_empty());
    }
}
