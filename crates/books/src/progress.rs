//! Reading Progress, kept as one plain value (ADR-0004): `encode` and `decode`
//! store it as one line, `<page_index> <x> <y> <last_read>` for a PDF and
//! `e <spine_index> <offset> <last_read>` for an EPUB (ADR-0006).

use std::cmp::Ordering;

/// A spot in a Document, in the terms of its Format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Place {
    /// A point on a page of a PDF.
    Pdf {
        /// Zero-based index of the page.
        page_index: u32,
        /// The point on that page in PDF page space (points, origin at the
        /// page's bottom-left).
        x: f64,
        y: f64,
    },
    /// A character of an EPUB's text.
    Epub {
        /// Zero-based index of the spine item.
        spine_index: u32,
        /// Offset into that item's text, in UTF-16 code units (ADR-0006).
        offset: u64,
    },
}

impl Place {
    /// Whether this Place comes no later than `other` in reading order: an
    /// earlier page or spine item, or the same one and no later. PDF points less
    /// than a point apart count as the same, so rounding in scrolling can't push
    /// a heading that was just jumped to out of reach. Pages read in one column,
    /// so horizontal position is ignored. Places of different kinds have no order.
    pub(crate) fn is_at_or_before(self, other: Place) -> bool {
        match (self, other) {
            (
                Place::Pdf { page_index, y, .. },
                Place::Pdf {
                    page_index: other_page,
                    y: other_y,
                    ..
                },
            ) if page_index == other_page => y >= other_y - 1.0,
            (Place::Pdf { .. }, Place::Pdf { .. }) | (Place::Epub { .. }, Place::Epub { .. }) => {
                self.cmp_reading(other).is_le()
            }
            _ => false,
        }
    }

    /// Whether this Place is the same spot as `other`: neither comes before the
    /// other.
    pub(crate) fn is_same_spot(self, other: Place) -> bool {
        self.is_at_or_before(other) && other.is_at_or_before(self)
    }

    /// The order of this Place and `other` in reading order, for sorting: by page
    /// or spine item, then top to bottom. `is_at_or_before` is this order, but
    /// for its tolerance.
    pub(crate) fn cmp_reading(self, other: Place) -> Ordering {
        match (self, other) {
            (
                Place::Pdf { page_index, y, .. },
                Place::Pdf {
                    page_index: other_page,
                    y: other_y,
                    ..
                },
            ) => page_index.cmp(&other_page).then(other_y.total_cmp(&y)),
            (
                Place::Epub {
                    spine_index,
                    offset,
                },
                Place::Epub {
                    spine_index: other_spine,
                    offset: other_offset,
                },
            ) => spine_index
                .cmp(&other_spine)
                .then(offset.cmp(&other_offset)),
            (Place::Pdf { .. }, Place::Epub { .. }) => Ordering::Less,
            (Place::Epub { .. }, Place::Pdf { .. }) => Ordering::Greater,
        }
    }

    /// Whether this Place is about a page or more from `other`: two PDF pages
    /// apart, or an EPUB Chapter or some 2000 characters apart. Places of
    /// different kinds are not.
    pub(crate) fn is_far(self, other: Place) -> bool {
        match (self, other) {
            (
                Place::Pdf { page_index, .. },
                Place::Pdf {
                    page_index: other, ..
                },
            ) => page_index.abs_diff(other) >= 2,
            (
                Place::Epub {
                    spine_index,
                    offset,
                },
                Place::Epub {
                    spine_index: other_spine,
                    offset: other_offset,
                },
            ) => spine_index != other_spine || offset.abs_diff(other_offset) > 2000,
            _ => false,
        }
    }
}

/// Where in a Document the user last left off, and when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReadingProgress {
    pub place: Place,
    /// Last Read: when the Reader showed the Document, in milliseconds since
    /// the Unix epoch.
    pub last_read: u64,
}

impl ReadingProgress {
    /// Progress at a point of a PDF page.
    #[cfg(test)]
    pub(crate) fn pdf(page_index: u32, x: f64, y: f64, last_read: u64) -> Self {
        let place = Place::Pdf { page_index, x, y };
        Self { place, last_read }
    }

    /// Progress at a character of an EPUB spine item.
    #[cfg(test)]
    pub(crate) fn epub(spine_index: u32, offset: u64, last_read: u64) -> Self {
        let place = Place::Epub {
            spine_index,
            offset,
        };
        Self { place, last_read }
    }
}

/// `progress` as stored.
pub(crate) fn encode(progress: &ReadingProgress) -> Vec<u8> {
    let last_read = progress.last_read;
    match progress.place {
        Place::Pdf { page_index, x, y } => format!("{page_index} {x} {y} {last_read}"),
        Place::Epub {
            spine_index,
            offset,
        } => format!("e {spine_index} {offset} {last_read}"),
    }
    .into_bytes()
}

/// The Reading Progress stored in `bytes`; none if they aren't one.
pub(crate) fn decode(bytes: &[u8]) -> Option<ReadingProgress> {
    let mut fields = std::str::from_utf8(bytes).ok()?.split(' ');
    let first = fields.next()?;
    let place = if first == "e" {
        Place::Epub {
            spine_index: fields.next()?.parse().ok()?,
            offset: fields.next()?.parse().ok()?,
        }
    } else {
        Place::Pdf {
            page_index: first.parse().ok()?,
            x: fields.next()?.parse().ok()?,
            y: fields.next()?.parse().ok()?,
        }
    };
    let last_read = fields.next()?.parse().ok()?;
    fields
        .next()
        .is_none()
        .then_some(ReadingProgress { place, last_read })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reads_back_as_encoded_and_anything_else_reads_as_none() {
        let pdf = ReadingProgress::pdf(7, 0.1 + 0.2, -3.75, u64::MAX);
        let epub = ReadingProgress::epub(3, u64::MAX, u64::MAX);
        for exact in [pdf, epub] {
            assert_eq!(decode(&encode(&exact)), Some(exact));
        }
        let bad_ones = [&b""[..], b"1 2 3", b"1 2 3 4 5", b"a 2 3 4", &[0xff]];
        let bad_epubs = [&b"e 1 2"[..], b"e 1 2 3 4", b"e -1 2 3", b"e 1 2.5 3"];
        for bad in bad_ones.into_iter().chain(bad_epubs) {
            assert_eq!(decode(bad), None, "{bad:?}");
        }
    }

    fn pdf(page_index: u32, y: f64) -> Place {
        Place::Pdf {
            page_index,
            x: 0.0,
            y,
        }
    }

    fn epub(spine_index: u32, offset: u64) -> Place {
        Place::Epub {
            spine_index,
            offset,
        }
    }

    #[test]
    fn epub_places_read_in_spine_order_then_by_offset() {
        let early = epub(1, 500);
        let late = epub(2, 0);
        assert!(early.is_at_or_before(late));
        assert!(!late.is_at_or_before(early));
        assert!(late.is_at_or_before(epub(2, 10)));
        assert!(!epub(2, 10).is_at_or_before(late));
    }

    #[test]
    fn epub_places_are_the_same_spot_only_at_the_same_offset() {
        let place = epub(3, 42);
        assert!(place.is_same_spot(epub(3, 42)));
        assert!(!place.is_same_spot(epub(3, 43)));
        assert!(!place.is_same_spot(epub(4, 42)));
    }

    #[test]
    fn pdf_places_read_by_page_then_down_the_page() {
        let top = pdf(1, 700.0);
        let lower = pdf(1, 300.0);
        assert!(top.is_at_or_before(lower));
        assert!(!lower.is_at_or_before(top));
        assert!(lower.is_at_or_before(pdf(2, 800.0)));
    }

    #[test]
    fn pdf_places_less_than_a_point_apart_are_the_same_spot() {
        let place = pdf(0, 500.0);
        let hair_below = Place::Pdf {
            page_index: 0,
            x: 30.0,
            y: 499.5,
        };
        assert!(place.is_same_spot(hair_below));
        assert!(!place.is_same_spot(pdf(0, 498.0)));
    }

    #[test]
    fn places_of_different_formats_have_no_order() {
        let (pdf, epub) = (pdf(0, 0.0), epub(0, 0));
        assert!(!pdf.is_at_or_before(epub));
        assert!(!epub.is_at_or_before(pdf));
        assert!(!pdf.is_same_spot(epub));
    }

    #[test]
    fn places_are_far_two_pdf_pages_or_an_epub_chapter_or_2000_characters_apart() {
        assert!(!pdf(3, 0.0).is_far(pdf(4, 0.0)));
        assert!(pdf(3, 0.0).is_far(pdf(5, 0.0)));
        assert!(!epub(1, 0).is_far(epub(1, 2000)));
        assert!(epub(1, 0).is_far(epub(1, 2001)));
        assert!(epub(1, 0).is_far(epub(2, 0)));
        assert!(!pdf(0, 0.0).is_far(epub(0, 0)));
    }
}
