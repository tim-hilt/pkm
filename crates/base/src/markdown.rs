//! Markdown in Tasks and Notes (ADR-0028): hand-written, no I/O. For now only
//! **Inline formatting** of a Title: emphasis `*…*`, strong `**…**` and code
//! `` `…` ``. Everything else, block syntax included, stays literal text.
//!
//! Ranges are in UTF-16 code units, which is how Swift's text views and
//! JavaScript count.

/// What an inline span is formatted as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inline {
    Emphasis,
    Strong,
    Code,
}

/// A formatted span of a text. It covers `start..end` with its markers; the
/// opening marker is the first `marker_len` units and the closing marker the
/// last `marker_len`, so a client hides or shows exactly those.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InlineSpan {
    pub kind: Inline,
    pub start: u32,
    pub end: u32,
    pub marker_len: u32,
}

/// The Inline formatting in a Title, outer spans before the ones inside them.
/// Unmatched markers (`2 * 3`) and everything that is not Inline formatting
/// (headings, links, images) are plain text and yield no span.
pub fn parse_title(text: &str) -> Vec<InlineSpan> {
    let chars: Vec<char> = text.chars().collect();
    // `at[i]` is the UTF-16 position of char `i`; `at[len]` the text's length.
    let at: Vec<u32> = chars
        .iter()
        .scan(0, |n, c| {
            let here = *n;
            *n += c.len_utf16() as u32;
            Some(here)
        })
        .chain(Some(chars.iter().map(|c| c.len_utf16() as u32).sum()))
        .collect();

    // Char indices: (kind, start, end, marker_len). Code spans are skipped
    // whole, so no span can cross one.
    let mut spans = Vec::new();
    let mut openers: Vec<Opener> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let run = run_at(&chars, i);
        match chars[i] {
            '`' => {
                // A code span ends at the next backtick run of the same length.
                match closing_backticks(&chars, i + run, run) {
                    Some(close) => {
                        spans.push((Inline::Code, i, close + run, run));
                        i = close + run;
                    }
                    None => i += run,
                }
            }
            '*' => {
                let can_open = chars.get(i + run).is_some_and(|c| !c.is_whitespace());
                let can_close = i > 0 && !chars[i - 1].is_whitespace();
                let mut used = 0; // closing markers already matched
                while can_close && used < run {
                    let Some(at_opener) = openers.pop() else {
                        break;
                    };
                    let n = if at_opener.left >= 2 && run - used >= 2 {
                        2
                    } else {
                        1
                    };
                    let open = at_opener.start + at_opener.left - n;
                    let close = i + used;
                    let kind = if n == 2 {
                        Inline::Strong
                    } else {
                        Inline::Emphasis
                    };
                    spans.push((kind, open, close + n, n));
                    used += n;
                    if at_opener.left > n {
                        openers.push(Opener {
                            left: at_opener.left - n,
                            ..at_opener
                        });
                    }
                }
                if can_open && used < run {
                    openers.push(Opener {
                        start: i + used,
                        left: run - used,
                    });
                }
                i += run;
            }
            _ => i += 1,
        }
    }

    let mut out: Vec<InlineSpan> = spans
        .iter()
        .map(|&(kind, start, end, marker)| InlineSpan {
            kind,
            start: at[start],
            end: at[end],
            marker_len: marker as u32,
        })
        .collect();
    out.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));
    out
}

/// A run of `*` that may still be closed by a later run; `left` of its
/// markers are unmatched, the innermost ones used first.
#[derive(Clone, Copy)]
struct Opener {
    start: usize,
    left: usize,
}

/// How many times the char at `i` repeats.
fn run_at(chars: &[char], i: usize) -> usize {
    chars[i..].iter().take_while(|&&c| c == chars[i]).count()
}

/// Where the first backtick run of exactly `len` at or after `from` starts.
fn closing_backticks(chars: &[char], from: usize, len: usize) -> Option<usize> {
    let mut i = from;
    while i < chars.len() {
        if chars[i] == '`' {
            let run = run_at(chars, i);
            if run == len {
                return Some(i);
            }
            i += run;
        } else {
            i += 1;
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(kind: Inline, start: u32, end: u32, marker_len: u32) -> InlineSpan {
        InlineSpan {
            kind,
            start,
            end,
            marker_len,
        }
    }

    #[test]
    fn plain_text_has_no_spans() {
        assert!(parse_title("Fix the login").is_empty());
        assert!(parse_title("").is_empty());
    }

    #[test]
    fn emphasis() {
        assert_eq!(parse_title("a *b* c"), [span(Inline::Emphasis, 2, 5, 1)]);
    }

    #[test]
    fn strong() {
        assert_eq!(parse_title("**b** c"), [span(Inline::Strong, 0, 5, 2)]);
    }

    #[test]
    fn code() {
        assert_eq!(parse_title("run `ls -a`"), [span(Inline::Code, 4, 11, 1)]);
    }

    #[test]
    fn code_may_use_longer_backtick_runs() {
        assert_eq!(parse_title("``a`b``"), [span(Inline::Code, 0, 7, 2)]);
    }

    #[test]
    fn markers_inside_code_are_literal() {
        assert_eq!(parse_title("`*a*`"), [span(Inline::Code, 0, 5, 1)]);
    }

    #[test]
    fn strong_inside_emphasis_nests_for_three_markers() {
        assert_eq!(
            parse_title("***x***"),
            [
                span(Inline::Emphasis, 0, 7, 1),
                span(Inline::Strong, 1, 6, 2)
            ]
        );
    }

    #[test]
    fn spans_nest_and_follow_each_other() {
        assert_eq!(
            parse_title("*a `b` c* **d**"),
            [
                span(Inline::Emphasis, 0, 9, 1),
                span(Inline::Code, 3, 6, 1),
                span(Inline::Strong, 10, 15, 2),
            ]
        );
    }

    #[test]
    fn unmatched_markers_stay_literal() {
        for text in [
            "2 * 3",
            "*a",
            "a*",
            "**a",
            "`a",
            "a * b * c",
            "****",
            "* a *",
        ] {
            assert!(parse_title(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn block_and_link_syntax_stays_literal() {
        for text in [
            "# Heading",
            "- item",
            "> quote",
            "[a](b)",
            "![](x.png)",
            "1. one",
        ] {
            assert!(parse_title(text).is_empty(), "{text}");
        }
    }

    #[test]
    fn emphasis_cannot_end_inside_code() {
        assert_eq!(parse_title("*a `b*` c"), [span(Inline::Code, 3, 7, 1)]);
    }

    #[test]
    fn ranges_count_utf16_code_units() {
        // 😀 is two units.
        assert_eq!(parse_title("😀 *é* 😀"), [span(Inline::Emphasis, 3, 6, 1)]);
        assert_eq!(parse_title("*😀*"), [span(Inline::Emphasis, 0, 4, 1)]);
    }
}
