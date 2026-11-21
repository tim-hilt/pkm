//! Search's matching (Notes and Bookmarks GLOSSARY.md): a query found in a
//! text ignoring case and accents, anywhere in a word, in the text as typed
//! (Markdown markers included). Positions are UTF-16 units, which is what
//! Swift and JavaScript count in.

use std::ops::Range;
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// How much text a snippet keeps before its match and after it, in UTF-16 units.
const BEFORE: usize = 40;
const AFTER: usize = 80;

/// A query ready to look for: folded once, for every text.
pub struct Needle(Vec<char>);

/// The text around a match, on one line, for telling results apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    pub text: String,
    /// The match in `text`, in UTF-16 units; empty where `text` is no match.
    pub matched: Range<usize>,
}

impl Needle {
    /// The query without the white space around it; `None` if nothing is left,
    /// which matches nothing.
    pub fn new(query: &str) -> Option<Self> {
        let folded = fold(query.trim());
        (!folded.is_empty()).then_some(Self(folded))
    }

    /// The snippet around the first match in `text`, if there is one.
    pub fn find(&self, text: &str) -> Option<Snippet> {
        let chars: Vec<char> = text.chars().collect();
        let range = self.char_matches(&chars).next()?;
        Some(snippet(&chars, range))
    }

    /// Every match in `text`, in order and apart, as ranges of UTF-16 units
    /// of `text` as written.
    pub fn find_all(&self, text: &str) -> Vec<Range<usize>> {
        let chars: Vec<char> = text.chars().collect();
        let mut starts = Vec::with_capacity(chars.len() + 1); // UTF-16 offset of each char
        let mut units = 0;
        for c in &chars {
            starts.push(units);
            units += c.len_utf16();
        }
        starts.push(units);
        (self.char_matches(&chars))
            .map(|r| starts[r.start]..starts[r.end])
            .collect()
    }

    /// The matches in `chars` as ranges of chars, in order and apart. A match
    /// covers every char whose folding it touches, so "ss" in "Straße" is the "ß".
    fn char_matches<'a>(&'a self, chars: &'a [char]) -> impl Iterator<Item = Range<usize>> + 'a {
        let mut folded = Vec::new();
        let mut source = Vec::new(); // `source[i]`: the char of `chars` that `folded[i]` comes from
        for (i, c) in chars.iter().enumerate() {
            for f in fold_char(*c) {
                folded.push(f);
                source.push(i);
            }
        }
        let mut from = 0;
        std::iter::from_fn(move || {
            let at = from + (folded[from..].windows(self.0.len())).position(|w| w == self.0)?;
            from = at + self.0.len();
            Some(source[at]..source[from - 1] + 1)
        })
    }
}

/// The snippet at the start of `text`, for a result whose match is elsewhere.
pub fn start_of(text: &str) -> Snippet {
    snippet(&text.chars().collect::<Vec<_>>(), 0..0)
}

/// `text` with case and accents gone.
fn fold(text: &str) -> Vec<char> {
    text.chars().flat_map(fold_char).collect()
}

/// What `c` folds to: its base letters, uppercased and lowercased again so
/// that "ß" is "ss".
fn fold_char(c: char) -> impl Iterator<Item = char> {
    (c.nfd().filter(|c| !is_combining_mark(*c)))
        .flat_map(char::to_uppercase)
        .flat_map(char::to_lowercase)
}

/// The chars of `chars` around the chars `matched`: up to `BEFORE` UTF-16 units
/// before and `AFTER` after, each run of white space made one space and none
/// at either end.
fn snippet(chars: &[char], matched: Range<usize>) -> Snippet {
    let width = |range: Range<usize>| chars[range].iter().map(|c| c.len_utf16()).sum::<usize>();
    let mut start = matched.start;
    while start > 0 && width(start - 1..matched.start) <= BEFORE {
        start -= 1;
    }
    let mut end = matched.end;
    while end < chars.len() && width(matched.end..end + 1) <= AFTER {
        end += 1;
    }
    let (mut text, mut units, mut space) = (String::new(), 0, false);
    let (mut from, mut to) = (None, None);
    for (i, c) in chars.iter().enumerate().take(end).skip(start) {
        if i == matched.start {
            from = Some(units + usize::from(space && !text.is_empty()));
        }
        if i == matched.end {
            to = Some(units);
        }
        if c.is_whitespace() {
            space |= !text.is_empty();
            continue;
        }
        if space {
            text.push(' ');
            units += 1;
            space = false;
        }
        text.push(*c);
        units += c.len_utf16();
    }
    let from = from.unwrap_or(units);
    Snippet {
        text,
        matched: from..to.unwrap_or(units).max(from),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(query: &str, text: &str) -> Option<Snippet> {
        Needle::new(query)?.find(text)
    }

    /// What `matched` marks in the snippet's text, counting UTF-16 units.
    fn marked(snippet: &Snippet) -> String {
        let units: Vec<u16> = snippet.text.encode_utf16().collect();
        String::from_utf16(&units[snippet.matched.clone()]).unwrap()
    }

    #[test]
    fn case_and_accents_are_ignored_both_ways() {
        assert!(found("cafe", "Un Café noir").is_some());
        assert!(found("CAFÉ", "un cafe").is_some());
        assert!(found("cafe\u{301}", "CAFE").is_some());
        assert!(found("strasse", "Straße").is_some());
        assert!(found("x", "Café").is_none());
    }

    #[test]
    fn a_match_may_be_inside_a_word_and_in_markers() {
        assert_eq!(marked(&found("graph", "a paragraph").unwrap()), "graph");
        assert_eq!(marked(&found("word", "a **word** b").unwrap()), "word");
        assert_eq!(marked(&found("**word", "a **word** b").unwrap()), "**word");
    }

    #[test]
    fn every_match_is_a_range_in_utf16_units_of_the_text_as_written() {
        let all = |query: &str, text: &str| Needle::new(query).unwrap().find_all(text);
        assert_eq!(all("tea", "😀 Tea, tea; TEA"), [3..6, 8..11, 13..16]);
        assert_eq!(all("aa", "aaaaa"), [0..2, 2..4]);
        assert_eq!(all("cafe", "Café **café**"), [0..4, 7..11]);
        assert_eq!(all("ss", "Straße").len(), 1);
        assert_eq!(all("ss", "Straße")[0], 4..5);
        assert!(all("x", "none").is_empty());
    }

    #[test]
    fn a_blank_query_matches_nothing() {
        assert!(Needle::new("").is_none());
        assert!(Needle::new(" \n").is_none());
        assert!(found(" b ", "a b c").is_some());
    }

    #[test]
    fn the_match_marks_the_text_as_written_not_as_folded() {
        assert_eq!(marked(&found("cafe", "Un Café noir").unwrap()), "Café");
        assert_eq!(marked(&found("ss", "Straße").unwrap()), "ß");
    }

    #[test]
    fn ranges_count_utf16_units_after_an_emoji() {
        let snippet = found("tea", "😀😀 tea").unwrap();
        assert_eq!(snippet.text, "😀😀 tea");
        assert_eq!(snippet.matched, 5..8);
        assert_eq!(marked(&snippet), "tea");
    }

    #[test]
    fn a_snippet_is_one_trimmed_line_cut_around_the_match() {
        let snippet = found("needle", "\n\nline  one\n\n  needle  here\n").unwrap();
        assert_eq!(snippet.text, "line one needle here");
        assert_eq!(marked(&snippet), "needle");
        let long = format!("{}needle{}", "a".repeat(100), "b".repeat(200));
        let snippet = found("needle", &long).unwrap();
        assert_eq!(snippet.text.len(), BEFORE + 6 + AFTER);
        assert_eq!(snippet.matched, BEFORE..BEFORE + 6);
    }

    #[test]
    fn a_cut_never_splits_an_emoji() {
        let long = format!("{}x{}", "😀".repeat(50), "😀".repeat(50));
        let snippet = found("x", &long).unwrap();
        assert_eq!(marked(&snippet), "x");
        assert_eq!(snippet.text.encode_utf16().count(), 40 + 1 + 80);
    }

    #[test]
    fn the_start_of_a_text_has_no_match() {
        let snippet = start_of("  first\nsecond");
        assert_eq!(
            (snippet.text.as_str(), snippet.matched),
            ("first second", 0..0)
        );
    }
}
