//! Soft wrapping of text, a port of Excalidraw's `textWrapping.ts` at the
//! pinned commit: where a line may break, and how a line is filled.
//!
//! Excalidraw finds break opportunities with Unicode regular expressions; the
//! core has no dependencies (ADR-0009), so the same rules are written out
//! over `char`s. Script classes are approximated by the main ranges of Han,
//! Hiragana, Katakana and Hangul. Emoji sequences are not kept together
//! (their characters never break apart anyway unless they are the only
//! thing on a line).

use std::collections::HashMap;

use crate::text::TextMeasurer;

/// Lays out `text` (hard line breaks as `\n`) in lines at most `max_width`
/// wide; a line that cannot be broken may be wider. A `max_width` that is not
/// a number or is negative only keeps the hard breaks.
pub(crate) fn wrap_text(
    text: &str,
    font_size: f64,
    max_width: f64,
    measurer: &dyn TextMeasurer,
) -> Vec<String> {
    if !max_width.is_finite() || max_width < 0.0 {
        return text.split('\n').map(str::to_owned).collect();
    }
    let mut measure = Measure::new(measurer, font_size);
    let mut lines = Vec::new();
    for line in text.split('\n') {
        if measure.width(line) <= max_width {
            lines.push(line.to_owned());
        } else {
            wrap_line(line, max_width, &mut measure, &mut lines);
        }
    }
    lines
}

/// The width of the longest of `lines` (an empty one counting as a space, as
/// Excalidraw measures it).
pub(crate) fn widest(lines: &[String], font_size: f64, measurer: &dyn TextMeasurer) -> f64 {
    lines
        .iter()
        .map(|line| measurer.line_width(if line.is_empty() { " " } else { line }, font_size))
        .fold(0.0, f64::max)
}

/// Widths of lines and, remembered, of single characters.
struct Measure<'a> {
    measurer: &'a dyn TextMeasurer,
    font_size: f64,
    chars: HashMap<char, f64>,
}

impl<'a> Measure<'a> {
    fn new(measurer: &'a dyn TextMeasurer, font_size: f64) -> Self {
        Self {
            measurer,
            font_size,
            chars: HashMap::new(),
        }
    }

    fn width(&self, text: &str) -> f64 {
        self.measurer.line_width(text, self.font_size)
    }

    fn char_width(&mut self, c: char) -> f64 {
        let (measurer, font_size) = (self.measurer, self.font_size);
        *self
            .chars
            .entry(c)
            .or_insert_with(|| measurer.line_width(c.encode_utf8(&mut [0; 4]), font_size))
    }
}

/// Wraps one hard line that is too wide (`wrapLine`).
fn wrap_line(line: &str, max_width: f64, measure: &mut Measure, lines: &mut Vec<String>) {
    let tokens = tokens(line);
    let mut current = String::new();
    let mut current_width = 0.0;
    let mut next = 0;

    while next < tokens.len() {
        let token = tokens[next];
        let test_width = match single_unit(token) {
            // Kerning must not apply to a lone character.
            Some(c) => current_width + measure.char_width(c),
            None => measure.width(&format!("{current}{token}")),
        };

        // Build up the line, whatever the width for whitespace: it may
        // trail.
        if token.chars().any(char::is_whitespace) || test_width <= max_width {
            current.push_str(token);
            current_width = test_width;
            next += 1;
        } else if current.is_empty() {
            // The word alone is too long for a line.
            let mut pieces = wrap_word(token, max_width, measure);
            let last = pieces.pop().unwrap_or_default();
            lines.append(&mut pieces);
            current_width = measure.width(&last);
            current = last;
            next += 1;
        } else {
            lines.push(current.trim_end().to_owned());
            current.clear();
            current_width = 0.0;
        }
    }
    if !current.is_empty() {
        lines.push(trim_line(&current, max_width, measure));
    }
}

/// Breaks a word that is too long for a line between characters
/// (`wrapWord`).
fn wrap_word(word: &str, max_width: f64, measure: &mut Measure) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut current_width = 0.0;
    for c in word.chars() {
        let width = measure.char_width(c);
        if current_width + width <= max_width {
            current.push(c);
            current_width += width;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        current.push(c);
        current_width = width;
    }
    if !current.is_empty() {
        lines.push(current);
    }
    lines
}

/// Drops the trailing whitespace of the last line of a hard line that does
/// not fit, keeping as much of it as fits (`trimLine`).
fn trim_line(line: &str, max_width: f64, measure: &mut Measure) -> String {
    if measure.width(line) <= max_width {
        return line.to_owned();
    }
    let body = line.trim_end();
    let trailing = &line[body.len()..];
    // Like `/^(.+?)(\s+)$/`, the first character is never trailing.
    let (body, trailing) = match body.chars().next() {
        None if !trailing.is_empty() => {
            trailing.split_at(trailing.chars().next().map_or(0, char::len_utf8))
        }
        _ => (body, trailing),
    };
    let mut kept = body.to_owned();
    let mut width = measure.width(body);
    for c in trailing.chars() {
        width += measure.char_width(c);
        if width > max_width {
            break;
        }
        kept.push(c);
    }
    kept
}

/// The character if `token` is one UTF-16 code unit: the tokens whose width
/// is the sum of its parts.
fn single_unit(token: &str) -> Option<char> {
    let mut chars = token.chars();
    let c = chars.next()?;
    (chars.next().is_none() && c.len_utf16() == 1).then_some(c)
}

// --- Break opportunities (`parseTokens`) -------------------------------------

/// Splits `line` where it may break.
fn tokens(line: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = 0;
    let mut previous: Option<char> = None;
    for (at, c) in line.char_indices() {
        if let Some(before) = previous
            && may_break(before, c)
        {
            tokens.push(&line[start..at]);
            start = at;
        }
        previous = Some(c);
    }
    if start < line.len() {
        tokens.push(&line[start..]);
    }
    tokens
}

/// Whether a line may break between `a` and `b`.
fn may_break(a: char, b: char) -> bool {
    // Around whitespace, and after hyphens.
    b.is_whitespace()
        || a.is_whitespace()
        || a == '-'
        // Before CJK characters and currency, unless they follow an opening
        // symbol.
        || ((is_cjk(b) || is_cjk_currency(b)) && !(is_opening(a) || is_cjk_opening(a)))
        // After CJK characters, unless a hyphen or closing symbol follows.
        || (is_cjk(a) && !(b == '-' || is_closing(b) || is_cjk_closing(b)))
        // Before a run of CJK opening symbols.
        || (is_cjk_opening(b) && !is_cjk_opening(a) && !is_opening(a))
        // After a run of CJK closing symbols.
        || (is_cjk_closing(a) && !is_cjk_closing(b) && !is_closing(b))
        // Between a closing and an opening symbol.
        || (is_closing(a) && is_opening(b))
}

fn is_opening(c: char) -> bool {
    matches!(c, '<' | '(' | '[' | '{')
}

fn is_closing(c: char) -> bool {
    matches!(
        c,
        '>' | ')' | ']' | '}' | '.' | ',' | ':' | ';' | '!' | '?' | '…' | '/'
    )
}

fn is_cjk_opening(c: char) -> bool {
    "（［｛〈《｟｢「『【〖〔〘〚＜〝".contains(c)
}

fn is_cjk_closing(c: char) -> bool {
    "）］｝〉》｠｣」』】〗〕〙〛＞。．，、〟‥？！：；・〜〞".contains(c)
}

fn is_cjk_currency(c: char) -> bool {
    "￥￦￡￠＄".contains(c)
}

/// Han, Hiragana, Katakana and Hangul letters, and the symbols Excalidraw
/// counts with them.
fn is_cjk(c: char) -> bool {
    matches!(c as u32,
        0x1100..=0x11FF       // Hangul Jamo
        | 0x2E80..=0x2FDF     // CJK radicals
        | 0x3005 | 0x3007 | 0x3021..=0x3029 | 0x3038..=0x303B
        | 0x3040..=0x309F     // Hiragana
        | 0x30A0..=0x30FF     // Katakana
        | 0x3130..=0x318F     // Hangul compatibility Jamo
        | 0x31F0..=0x31FF     // Katakana extensions
        | 0x3400..=0x4DBF     // Han extension A
        | 0x4E00..=0x9FFF     // Han
        | 0xA960..=0xA97F     // Hangul Jamo extended A
        | 0xAC00..=0xD7FF     // Hangul syllables and Jamo extended B
        | 0xF900..=0xFAFF     // Han compatibility
        | 0xFF66..=0xFF9F     // half-width Katakana
        | 0x20000..=0x323AF   // Han extensions B and beyond
    ) || "｀＇＾〃〰〆＃＆＊＋－ー／＼＝｜￤〒￢￣".contains(c)
}
