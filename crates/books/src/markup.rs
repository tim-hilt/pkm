//! Markup of a Book: the shared model (`pkm_base::markup`, ADR-0034) with
//! this Context's anchors.
//!
//! The anchor of a PDF Highlight is a list of pages with their rectangles in PDF
//! page space (points, origin at the page's bottom-left), engine-neutral
//! (ADR-0001). That of an EPUB Highlight is a start and an end position in the
//! text (ADR-0006). The quoted text is the fallback of both.

use crate::json::{number, object};
use crate::progress::Place;
use pkm_base::json::Value;
pub use pkm_base::markup::Rgb;
use pkm_base::markup::{self, Located};

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

/// A passage of a Book's text marked in a color. It may run across pages. For
/// an EPUB `quoted_text` is the text from `start` to `end`, the spine items'
/// texts joined by `\n`.
pub type Highlight = markup::Highlight<Anchor>;

/// A Comment on a spot of a page, belonging to no passage; its `place` is in
/// the terms of its Book's Format.
pub type PageComment = markup::PageComment<Place>;

/// Everything the user added on top of a Book.
pub type Markup = markup::Markup<Anchor, Place>;

impl Located for Anchor {
    fn to_json(&self) -> Value {
        match self {
            Anchor::Pdf(pages) => object([(
                "pdf",
                Value::Array(
                    pages
                        .iter()
                        .map(|p| {
                            let rects = p.rects.iter().flat_map(|r| [r.x, r.y, r.width, r.height]);
                            object([
                                ("page_index", number(p.page_index)),
                                ("rects", Value::Array(rects.map(Value::Number).collect())),
                            ])
                        })
                        .collect(),
                ),
            )]),
            Anchor::Epub { start, end } => object([(
                "epub",
                object([
                    ("start_spine", number(start.spine_index)),
                    ("start_offset", number(start.offset)),
                    ("end_spine", number(end.spine_index)),
                    ("end_offset", number(end.offset)),
                ]),
            )]),
        }
    }

    fn from_json(anchor: &Value) -> Option<Self> {
        if let Some(pages) = anchor.get("pdf") {
            return Some(Anchor::Pdf(
                pages
                    .as_array()?
                    .iter()
                    .map(read_page_rects)
                    .collect::<Option<_>>()?,
            ));
        }
        let epub = anchor.get("epub")?;
        let position = |spine: &str, offset: &str| {
            Some(EpubPosition {
                spine_index: u32_at(epub, spine)?,
                offset: epub.get(offset)?.as_u64()?,
            })
        };
        Some(Anchor::Epub {
            start: position("start_spine", "start_offset")?,
            end: position("end_spine", "end_offset")?,
        })
    }
}

impl Located for Place {
    fn to_json(&self) -> Value {
        match *self {
            Place::Pdf { page_index, x, y } => object([(
                "pdf",
                object([
                    ("page_index", number(page_index)),
                    ("x", Value::Number(x)),
                    ("y", Value::Number(y)),
                ]),
            )]),
            Place::Epub {
                spine_index,
                offset,
            } => object([(
                "epub",
                object([("spine", number(spine_index)), ("offset", number(offset))]),
            )]),
        }
    }

    fn from_json(place: &Value) -> Option<Self> {
        if let Some(epub) = place.get("epub") {
            return Some(Place::Epub {
                spine_index: u32_at(epub, "spine")?,
                offset: epub.get("offset")?.as_u64()?,
            });
        }
        let pdf = place.get("pdf")?;
        Some(Place::Pdf {
            page_index: u32_at(pdf, "page_index")?,
            x: pdf.get("x")?.as_f64()?,
            y: pdf.get("y")?.as_f64()?,
        })
    }
}

fn u32_at(json: &Value, key: &str) -> Option<u32> {
    u32::try_from(json.get(key)?.as_u64()?).ok()
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

    fn written(markup: &Markup) -> String {
        markup.to_json().to_json()
    }

    fn read(json: &str) -> Markup {
        Markup::from_json(&pkm_base::json::parse(json).unwrap())
    }

    fn round_trip(markup: &Markup) -> Markup {
        read(&written(markup))
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
        let markup = read(
            r#"{
            "highlights": [
                { "id": "no-color", "anchor": { "epub": {} }, "quoted_text": "x", "created": 1 },
                { "id": "ok", "anchor": { "epub": {
                    "start_spine": 0, "start_offset": 0, "end_spine": 0, "end_offset": 4 } },
                  "quoted_text": "x", "color": 16777215, "created": 1, "future": true }
            ],
            "page_comments": [{ "id": "no-place", "created": 1, "text": "t" }]
        }"#,
        );
        assert_eq!(markup.highlights.len(), 1);
        assert_eq!(markup.highlights[0].id, "ok");
        assert!(markup.page_comments.is_empty());
        assert_eq!(read("7"), Markup::default());
    }

    /// What `to_json` wrote before `pkm-base` replaced serde_json: keys sorted,
    /// compact. Saved Books carry it, so it must keep reading and writing.
    const MARKUP_JSON: &str = r#"{"highlights":[{"anchor":{"pdf":[{"page_index":2,"rects":[1.5,2.5,30.25,4.75]}]},"color":16771900,"comment":"why \"not\"\né","created":1,"id":"a","quoted_text":"quoted — “text”"},{"anchor":{"epub":{"end_offset":3,"end_spine":2,"start_offset":10,"start_spine":1}},"color":16771900,"created":2,"id":"b","quoted_text":"quoted — “text”"}],"page_comments":[{"created":3,"id":"c","place":{"pdf":{"page_index":4,"x":10.5,"y":20.25}},"text":"spot"},{"created":4,"id":"d","place":{"epub":{"offset":99,"spine":3}},"text":"epub spot"}]}"#;

    fn fixture_markup() -> Markup {
        let mut pdf = pdf_highlight("a", 1);
        pdf.comment = Some("why \"not\"\n\u{e9}".into());
        let Anchor::Pdf(pages) = &mut pdf.anchor else {
            unreachable!()
        };
        pages[0].rects[0].y = 2.5;
        pages[0].rects[0].height = 4.75;
        let mut spot = page_comment("c", 3, "spot");
        spot.place = Place::Pdf {
            page_index: 4,
            x: 10.5,
            y: 20.25,
        };
        Markup {
            highlights: vec![pdf, epub_highlight("b", 2)],
            page_comments: vec![
                spot,
                PageComment {
                    place: Place::Epub {
                        spine_index: 3,
                        offset: 99,
                    },
                    ..page_comment("d", 4, "epub spot")
                },
            ],
        }
    }

    #[test]
    fn markup_is_written_as_the_bytes_saved_books_hold() {
        assert_eq!(written(&fixture_markup()), MARKUP_JSON);
    }

    #[test]
    fn markup_is_read_from_the_bytes_saved_books_hold() {
        assert_eq!(read(MARKUP_JSON), fixture_markup());
    }

    #[test]
    fn whole_numbers_read_the_same_however_they_are_spelled() {
        let spelled = MARKUP_JSON.replace("2.5,", "2.0,").replace("20.25", "20.0");
        let mut expected = fixture_markup();
        expected.page_comments[0].place = Place::Pdf {
            page_index: 4,
            x: 10.5,
            y: 20.0,
        };
        let Anchor::Pdf(pages) = &mut expected.highlights[0].anchor else {
            unreachable!()
        };
        pages[0].rects[0].y = 2.0;
        assert_eq!(read(&spelled), expected);
    }
}
