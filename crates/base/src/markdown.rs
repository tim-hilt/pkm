//! Markdown in Tasks and Notes (ADR-0028): hand-written, no I/O. A Title takes
//! **Inline formatting** only: emphasis `*…*`, strong `**…**` and code
//! `` `…` ``. A Description also takes headings `#`–`###`, `- ` and `N. `
//! lists, quotes `> `, fenced code blocks, links and Images. Everything else
//! stays literal text.
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
    let at = positions(&chars);

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

/// `at[i]` is the UTF-16 position of char `i`; `at[len]` the text's length.
fn positions(chars: &[char]) -> Vec<u32> {
    let mut n = 0;
    let mut at: Vec<u32> = chars
        .iter()
        .map(|c| {
            let here = n;
            n += c.len_utf16() as u32;
            here
        })
        .collect();
    at.push(n);
    at
}

fn utf16_len(text: &str) -> u32 {
    text.chars().map(|c| c.len_utf16() as u32).sum()
}

/// What a block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Block {
    Paragraph,
    /// `#`–`###`; `level` is 1–3.
    Heading,
    /// `- `; `level` is the nesting depth, two spaces per level.
    Bullet,
    /// `N. `; `level` as for Bullet, `number` is the one to show.
    Numbered,
    Quote,
    /// Opening to closing fence, or to the text's end while unclosed.
    CodeFence,
    /// A line of its own, `![alt](target)`.
    Image,
}

/// A block of a Description. It covers `start..end` (a line, or all lines of
/// a fence) and its marker is `marker_start..marker_end` (empty for a
/// paragraph or an Image), which the clients hide on rendered lines. A fence
/// has two markers, its opening and closing lines, each with the line break
/// after it, so that hiding them hides the lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockSpan {
    pub kind: Block,
    pub start: u32,
    pub end: u32,
    pub marker_start: u32,
    pub marker_end: u32,
    pub level: u32,
    /// The number a Numbered item shows: counted up from its list's first.
    pub number: u32,
    /// An Image's reference.
    pub target: String,
    /// A fence's closing marker; empty at `end` for an unclosed fence and
    /// every other block.
    pub closing_start: u32,
    pub closing_end: u32,
}

/// A link `[text](target)`. Its markers are `start..text_start` and
/// `text_end..end`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkSpan {
    pub start: u32,
    pub end: u32,
    pub text_start: u32,
    pub text_end: u32,
    pub target: String,
}

/// A parsed Description: blocks in text order, with the Inline formatting and
/// links of their text, also in text order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Description {
    pub blocks: Vec<BlockSpan>,
    pub inlines: Vec<InlineSpan>,
    pub links: Vec<LinkSpan>,
}

/// A range of a text in UTF-16 code units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub start: u32,
    pub end: u32,
}

struct Line<'a> {
    start: u32,
    end: u32,
    text: &'a str,
}

fn lines(text: &str) -> Vec<Line<'_>> {
    let mut start = 0;
    text.split('\n')
        .map(|text| {
            let end = start + utf16_len(text);
            let line = Line { start, end, text };
            start = end + 1;
            line
        })
        .collect()
}

/// The block and its markers of a line outside a fence; markers are ASCII, so
/// bytes are UTF-16 units. Returns the kind, marker start, marker length,
/// level and number.
fn line_block(line: &str) -> (Block, usize, usize, u32, u32) {
    let indent = line.len() - line.trim_start_matches(' ').len();
    let rest = &line[indent..];
    let depth = (indent / 2) as u32;
    if indent == 0 {
        let hashes = rest.chars().take_while(|&c| c == '#').count();
        if (1..=3).contains(&hashes) && rest[hashes..].starts_with(' ') {
            return (Block::Heading, 0, hashes + 1, hashes as u32, 0);
        }
        if rest.starts_with("> ") {
            return (Block::Quote, 0, 2, 0, 0);
        }
    }
    if rest.starts_with("- ") {
        return (Block::Bullet, indent, 2, depth, 0);
    }
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if (1..=9).contains(&digits) && rest[digits..].starts_with(". ") {
        let number = rest[..digits].parse().unwrap_or(0);
        return (Block::Numbered, indent, digits + 2, depth, number);
    }
    (Block::Paragraph, 0, 0, 0, 0)
}

/// `![alt](target)` and nothing else: the target.
fn image_target(line: &str) -> Option<&str> {
    let inner = line.strip_prefix("![")?.strip_suffix(')')?;
    let (alt, target) = inner.split_once("](")?;
    (!alt.contains(']') && !target.is_empty() && !target.contains(')')).then_some(target)
}

fn is_fence_close(line: &str) -> bool {
    let t = line.trim();
    t.len() >= 3 && t.chars().all(|c| c == '`')
}

/// The numbers of the open lists, by indentation, so that a list counts up
/// from its first item.
#[derive(Default)]
struct Lists(Vec<(usize, bool, u32)>);

impl Lists {
    /// The number for an item at `indent`; `first` starts a list.
    fn number(&mut self, indent: usize, ordered: bool, first: u32) -> u32 {
        while self.0.last().is_some_and(|l| l.0 > indent) {
            self.0.pop();
        }
        match self.0.last_mut() {
            Some(l) if l.0 == indent && l.1 == ordered => {
                l.2 += 1;
                l.2 - 1
            }
            other => {
                if other.is_some_and(|l| l.0 == indent) {
                    self.0.pop();
                }
                self.0.push((indent, ordered, first + 1));
                first
            }
        }
    }
}

/// The blocks, Inline formatting and links of a Description. Text outside the
/// supported subset is a paragraph and yields no span.
pub fn parse_description(text: &str) -> Description {
    let lines = lines(text);
    let mut d = Description::default();
    let mut lists = Lists::default();
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        i += 1;
        if line.text.is_empty() {
            lists = Lists::default();
            continue;
        }
        let block = |kind, marker: (u32, u32), level, number, target: &str| BlockSpan {
            kind,
            start: line.start,
            end: line.end,
            marker_start: line.start + marker.0,
            marker_end: line.start + marker.1,
            level,
            number,
            target: target.to_string(),
            closing_start: line.end,
            closing_end: line.end,
        };
        if line.text.starts_with("```") {
            lists = Lists::default();
            let close = lines[i..].iter().position(|l| is_fence_close(l.text));
            let last = close.map_or(lines.len() - 1, |c| i + c);
            // A line's end with its line break, if it has one.
            let with_break = |l: usize| lines[l].end + u32::from(l + 1 < lines.len());
            let opening = with_break(i - 1) - line.start;
            let mut fence = block(Block::CodeFence, (0, opening), 0, 0, "");
            fence.end = lines[last].end;
            (fence.closing_start, fence.closing_end) = match close {
                Some(_) => (lines[last].start, with_break(last)),
                None => (fence.end, fence.end),
            };
            d.blocks.push(fence);
            i = last + 1;
            continue;
        }
        if let Some(target) = image_target(line.text) {
            lists = Lists::default();
            d.blocks.push(block(Block::Image, (0, 0), 0, 0, target));
            continue;
        }
        let (kind, from, len, level, number) = line_block(line.text);
        let number = match kind {
            Block::Bullet => {
                lists.number(from, false, 0);
                0
            }
            Block::Numbered => lists.number(from, true, number),
            _ => {
                lists = Lists::default();
                0
            }
        };
        let marker = (from as u32, (from + len) as u32);
        d.blocks.push(block(kind, marker, level, number, ""));
        inline_spans(&mut d, &line.text[from + len..], line.start + marker.1);
    }
    d
}

/// Adds the Inline formatting and links of `text`, which starts at `offset`.
/// The line a Task row shows for a Description: its first line with text, block
/// markers stripped, empty and Image-only lines skipped (a fence's lines count as
/// text, its markers not). Empty when there is none. Callers format it like a Title
/// (`parse_title`).
pub fn row_preview(text: &str) -> String {
    let mut in_fence = false;
    for line in text.split('\n') {
        if in_fence {
            if is_fence_close(line) {
                in_fence = false;
            } else if !line.is_empty() {
                return line.to_string();
            }
            continue;
        }
        if line.starts_with("```") {
            in_fence = true;
            continue;
        }
        if image_target(line).is_some() {
            continue;
        }
        let (_, from, len, ..) = line_block(line);
        let body = line[from + len..].trim_start_matches(' ');
        if !body.is_empty() {
            return body.to_string();
        }
    }
    String::new()
}

fn inline_spans(d: &mut Description, text: &str, offset: u32) {
    let spans = parse_title(text);
    let chars: Vec<char> = text.chars().collect();
    let at = positions(&chars);
    let in_code = |pos: u32| {
        spans
            .iter()
            .any(|s| s.kind == Inline::Code && (s.start..s.end).contains(&pos))
    };
    let mut i = 0;
    while i < chars.len() {
        let link = (chars[i] == '[' && (i == 0 || chars[i - 1] != '!') && !in_code(at[i]))
            .then(|| link_at(&chars, i))
            .flatten();
        match link {
            Some((close, end)) => {
                d.links.push(LinkSpan {
                    start: offset + at[i],
                    end: offset + at[end],
                    text_start: offset + at[i + 1],
                    text_end: offset + at[close],
                    target: chars[close + 2..end - 1].iter().collect(),
                });
                i = end;
            }
            None => i += 1,
        }
    }
    d.inlines.extend(spans.into_iter().map(|s| InlineSpan {
        start: s.start + offset,
        end: s.end + offset,
        ..s
    }));
}

/// For a link opening at char `open`: the chars of its `]` and its end.
fn link_at(chars: &[char], open: usize) -> Option<(usize, usize)> {
    let close = open
        + 1
        + chars[open + 1..]
            .iter()
            .position(|&c| c == ']' || c == '[')?;
    if chars[close] != ']' || close == open + 1 || chars.get(close + 1) != Some(&'(') {
        return None;
    }
    let end = close + 2 + chars[close + 2..].iter().position(|&c| c == ')')?;
    (end > close + 2).then_some((close, end + 1))
}

/// What a caret at `caret` shows raw: its line, or the whole fence it is in.
pub fn raw_range(text: &str, caret: u32) -> TextRange {
    let fence = parse_description(text)
        .blocks
        .into_iter()
        .find(|b| b.kind == Block::CodeFence && (b.start..=b.end).contains(&caret));
    if let Some(f) = fence {
        return TextRange {
            start: f.start,
            end: f.end,
        };
    }
    let lines = lines(text);
    let line = lines
        .iter()
        .find(|l| caret <= l.end)
        .unwrap_or(&lines[lines.len() - 1]);
    TextRange {
        start: line.start,
        end: line.end,
    }
}

/// A change to a text: `start..end` (UTF-16 code units) is replaced by
/// `replacement`, and the selection is `selection_start..selection_end` of the
/// text after the change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub start: u32,
    pub end: u32,
    pub replacement: String,
    pub selection_start: u32,
    pub selection_end: u32,
}

impl TextEdit {
    fn replace(start: u32, end: u32, replacement: String, caret: u32) -> Self {
        Self {
            start,
            end,
            replacement,
            selection_start: caret,
            selection_end: caret,
        }
    }
}

/// The lines of `text` that are list items outside a code fence, with their
/// indentation.
fn list_items(text: &str) -> Vec<(Line<'_>, usize)> {
    let fences: Vec<_> = parse_description(text)
        .blocks
        .into_iter()
        .filter(|b| b.kind == Block::CodeFence)
        .collect();
    lines(text)
        .into_iter()
        .filter(|l| !fences.iter().any(|f| (f.start..=f.end).contains(&l.start)))
        .filter_map(|l| {
            let (kind, indent, ..) = line_block(l.text);
            matches!(kind, Block::Bullet | Block::Numbered).then_some((l, indent))
        })
        .collect()
}

/// The edit for Return with the selection `start..end`. On a `- ` or `N. `
/// item it continues the list at the same indentation (`3.` → `4. `), or ends
/// it when the item is empty; anywhere else it is a plain line break.
pub fn return_at(text: &str, start: u32, end: u32) -> TextEdit {
    let item = list_items(text)
        .into_iter()
        .find(|(l, _)| (l.start..=l.end).contains(&start));
    if let Some((line, indent)) = item {
        let (kind, _, len, _, number) = line_block(line.text);
        let marker_end = (indent + len) as u32;
        if start - line.start >= marker_end {
            if start == end && line.text[indent + len..].trim().is_empty() {
                return TextEdit::replace(line.start, line.end, String::new(), line.start);
            }
            let marker = match kind {
                Block::Numbered => format!("{}. ", number + 1),
                _ => "- ".to_string(),
            };
            let replacement = format!("\n{}{marker}", " ".repeat(indent));
            let caret = start + utf16_len(&replacement);
            return TextEdit::replace(start, end, replacement, caret);
        }
    }
    TextEdit::replace(start, end, "\n".to_string(), start + 1)
}

/// The edit that indents (or, with `outdent`, outdents) by two spaces the list
/// items touched by the selection `start..end`. `None` when nothing changes.
pub fn shift_items(text: &str, start: u32, end: u32, outdent: bool) -> Option<TextEdit> {
    // A range ending at the start of a line doesn't touch that line.
    let last = if end > start { end - 1 } else { end };
    let items: Vec<_> = list_items(text)
        .into_iter()
        .filter(|(l, _)| l.end >= start && l.start <= last)
        .collect();
    let (from, to) = (items.first()?.0.start, items.last()?.0.end);
    let amounts: Vec<(u32, u32)> = items
        .iter()
        .map(|(l, indent)| (l.start, if outdent { (*indent).min(2) as u32 } else { 2 }))
        .collect();
    if amounts.iter().all(|&(_, n)| n == 0) {
        return None;
    }
    let mut replacement = Vec::new();
    for line in lines(text)
        .iter()
        .filter(|l| (from..=to).contains(&l.start))
    {
        replacement.push(match amounts.iter().find(|a| a.0 == line.start) {
            Some(&(_, n)) if outdent => line.text[n as usize..].to_string(),
            Some(_) => format!("  {}", line.text),
            None => line.text.to_string(),
        });
    }
    // A position moves with the items before it, and within its own line.
    let moved = |pos: u32| {
        amounts
            .iter()
            .filter(|a| a.0 <= pos)
            .fold(
                pos,
                |p, &(at, n)| if outdent { p - n.min(pos - at) } else { p + n },
            )
    };
    Some(TextEdit {
        start: from,
        end: to,
        replacement: replacement.join("\n"),
        selection_start: moved(start),
        selection_end: moved(end),
    })
}

/// The edit for ⌘B, ⌘I or ⌘E with the selection `start..end`: it removes the
/// `kind` markers of the span the selection is in (inside its markers) or is
/// exactly, else wraps the selection in them. Spaces at the selection's ends
/// are left out, as a span can't open or close on one; an empty selection gets
/// a pair with the caret between.
pub fn toggle_inline(text: &str, start: u32, end: u32, kind: Inline) -> TextEdit {
    let units: Vec<u16> = text.encode_utf16().collect();
    let end = end.min(units.len() as u32);
    let start = start.min(end);
    let blank = |u: u16| char::from_u32(u32::from(u)).is_some_and(char::is_whitespace);
    let (mut from, mut to) = (start as usize, end as usize);
    while from < to && blank(units[from]) {
        from += 1;
    }
    while to > from && blank(units[to - 1]) {
        to -= 1;
    }
    if from == to {
        (from, to) = (end as usize, end as usize);
    }
    let (trimmed_start, trimmed_end) = (from as u32, to as u32);
    let span = parse_description(text).inlines.into_iter().rev().find(|s| {
        let marker = s.marker_len;
        s.kind == kind
            && ((s.start + marker <= trimmed_start && trimmed_end <= s.end - marker)
                || (trimmed_start, trimmed_end) == (s.start, s.end))
    });
    if let Some(s) = span {
        let marker = s.marker_len;
        let inner = String::from_utf16_lossy(
            &units[(s.start + marker) as usize..(s.end - marker) as usize],
        );
        // A position moves left by the markers before it.
        let moved = |pos: u32| match pos {
            p if p <= s.start => p,
            p if p < s.start + marker => s.start,
            p if p <= s.end - marker => p - marker,
            p if p < s.end => s.end - 2 * marker,
            p => p - 2 * marker,
        };
        return TextEdit {
            start: s.start,
            end: s.end,
            replacement: inner,
            selection_start: moved(start),
            selection_end: moved(end),
        };
    }
    let marker = match kind {
        Inline::Emphasis => "*",
        Inline::Strong => "**",
        Inline::Code => "`",
    };
    let inner = String::from_utf16_lossy(&units[from..to]);
    let width = utf16_len(marker);
    TextEdit {
        start: from as u32,
        end: to as u32,
        replacement: format!("{marker}{inner}{marker}"),
        selection_start: from as u32 + width,
        selection_end: to as u32 + width,
    }
}

/// The references of the Images a text shows, in text order (fenced code shows
/// none).
pub fn image_targets(text: &str) -> Vec<String> {
    let description = parse_description(text);
    let images = description.blocks.iter().filter(|b| b.kind == Block::Image);
    images.map(|b| b.target.clone()).collect()
}

/// The edit that puts the Image `reference` on a line of its own, replacing the
/// selection `start..end`; the caret ends on the line after it.
pub fn insert_image(text: &str, start: u32, end: u32, reference: &str) -> TextEdit {
    let units: Vec<u16> = text.encode_utf16().collect();
    let end = end.min(units.len() as u32);
    let start = start.min(end);
    let newline = u16::from(b'\n');
    let before = start > 0 && units[start as usize - 1] != newline;
    let after = units.get(end as usize) != Some(&newline);
    let replacement = format!(
        "{}![]({reference}){}",
        if before { "\n" } else { "" },
        if after { "\n" } else { "" }
    );
    // Past an existing line break there is nothing to add.
    let caret = start + utf16_len(&replacement) + u32::from(!after);
    TextEdit::replace(start, end, replacement, caret)
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

    #[test]
    fn an_image_goes_on_a_line_of_its_own() {
        let apply = |text: &str, start, end| {
            let edit = insert_image(text, start, end, "a.png");
            let (s, e) = (edit.start as usize, edit.end as usize);
            let units: Vec<u16> = text.encode_utf16().collect();
            let mut out = String::from_utf16_lossy(&units[..s]);
            out += &edit.replacement;
            out += &String::from_utf16_lossy(&units[e..]);
            (out, edit.selection_start)
        };
        assert_eq!(apply("", 0, 0), ("![](a.png)\n".to_string(), 11));
        assert_eq!(apply("ab", 1, 1), ("a\n![](a.png)\nb".to_string(), 13));
        assert_eq!(apply("a\nb", 2, 2), ("a\n![](a.png)\nb".to_string(), 13));
        assert_eq!(apply("a\n\nb", 2, 2), ("a\n![](a.png)\nb".to_string(), 13));
        assert_eq!(apply("a😀b", 0, 3), ("![](a.png)\nb".to_string(), 11));
    }

    #[test]
    fn image_targets_lists_the_shown_images_in_order() {
        let text = "![](a.png)\ntext ![](no.png)\n```\n![](code.png)\n```\n![alt](b.jpg)";
        assert_eq!(image_targets(text), ["a.png", "b.jpg"]);
        assert!(image_targets("no images").is_empty());
    }

    #[test]
    fn row_preview_strips_block_markers() {
        assert_eq!(row_preview("# x"), "x");
        assert_eq!(row_preview("- x"), "x");
        assert_eq!(row_preview("  - x"), "x");
        assert_eq!(row_preview("3. x"), "x");
        assert_eq!(row_preview("> x"), "x");
        assert_eq!(row_preview("#x"), "#x");
    }

    #[test]
    fn row_preview_keeps_inline_markers_for_formatting() {
        assert_eq!(row_preview("- a **b**"), "a **b**");
    }

    #[test]
    fn row_preview_skips_images_and_empty_lines() {
        assert_eq!(row_preview("![](a.png)\n\n![](b.png)\n- x\ny"), "x");
        assert_eq!(row_preview("![](a.png)"), "");
        assert_eq!(row_preview("- \nx"), "x");
    }

    #[test]
    fn row_preview_of_empty_description_is_empty() {
        assert_eq!(row_preview(""), "");
        assert_eq!(row_preview("\n\n"), "");
    }

    #[test]
    fn row_preview_skips_fence_markers() {
        assert_eq!(row_preview("```rs\nlet x;\n```"), "let x;");
        assert_eq!(row_preview("```\n```\nafter"), "after");
    }

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

    fn block(kind: Block, start: u32, end: u32, marker: (u32, u32), level: u32) -> BlockSpan {
        BlockSpan {
            kind,
            start,
            end,
            marker_start: marker.0,
            marker_end: marker.1,
            level,
            number: 0,
            target: String::new(),
            closing_start: end,
            closing_end: end,
        }
    }

    fn blocks(text: &str) -> Vec<BlockSpan> {
        parse_description(text).blocks
    }

    #[test]
    fn paragraphs_and_empty_lines() {
        assert_eq!(
            blocks("a\n\nb"),
            [
                block(Block::Paragraph, 0, 1, (0, 0), 0),
                block(Block::Paragraph, 3, 4, (3, 3), 0)
            ]
        );
        assert!(blocks("").is_empty());
    }

    #[test]
    fn headings_one_to_three() {
        assert_eq!(blocks("## Hi"), [block(Block::Heading, 0, 5, (0, 3), 2)]);
        assert_eq!(blocks("# Hi")[0].level, 1);
        assert_eq!(blocks("### Hi")[0].level, 3);
    }

    #[test]
    fn syntax_outside_the_subset_is_a_paragraph() {
        for text in [
            "#### Hi",
            "#Hi",
            " # Hi",
            "* a",
            "1) a",
            "-a",
            ">a",
            "10 apples",
        ] {
            assert_eq!(blocks(text)[0].kind, Block::Paragraph, "{text}");
        }
    }

    #[test]
    fn quote() {
        assert_eq!(blocks("> q"), [block(Block::Quote, 0, 3, (0, 2), 0)]);
    }

    #[test]
    fn bullets_nest_by_two_spaces() {
        assert_eq!(
            blocks("- a\n  - b"),
            [
                block(Block::Bullet, 0, 3, (0, 2), 0),
                block(Block::Bullet, 4, 9, (6, 8), 1)
            ]
        );
    }

    fn numbers(text: &str) -> Vec<u32> {
        blocks(text).iter().map(|b| b.number).collect()
    }

    #[test]
    fn numbered_items_count_from_the_first() {
        assert_eq!(numbers("1. a\n1. b\n1. c"), [1, 2, 3]);
        assert_eq!(numbers("3. a\n1. b"), [3, 4]);
        assert_eq!(blocks("12. a")[0].marker_end, 4);
    }

    #[test]
    fn numbering_restarts_after_a_break_and_per_depth() {
        assert_eq!(numbers("1. a\ntext\n5. b"), [1, 0, 5]);
        assert_eq!(numbers("1. a\n  1. b\n  1. c\n1. d"), [1, 1, 2, 2]);
        assert_eq!(numbers("1. a\n- b\n1. c"), [1, 0, 1]);
        assert_eq!(numbers("- a\n- b"), [0, 0]);
    }

    #[test]
    fn fenced_code_is_one_block_without_inline_spans() {
        let d = parse_description("```rs\n*a*\n# b\n```\nafter");
        let fence = BlockSpan {
            closing_start: 14,
            closing_end: 18,
            ..block(Block::CodeFence, 0, 17, (0, 6), 0)
        };
        assert_eq!(d.blocks[0], fence);
        assert_eq!(d.blocks[1].kind, Block::Paragraph);
        assert!(d.inlines.is_empty());
    }

    #[test]
    fn fence_markers_take_their_line_breaks_along() {
        let markers = |text| {
            let b = &blocks(text)[0];
            (b.marker_start, b.marker_end, b.closing_start, b.closing_end)
        };
        assert_eq!(markers("```\na\n```"), (0, 4, 6, 9));
        assert_eq!(markers("```\n```"), (0, 4, 4, 7));
    }

    #[test]
    fn unclosed_fence_runs_to_the_end() {
        let b = &blocks("```\na\nb")[0];
        assert_eq!(
            (b.end, b.marker_end, b.closing_start, b.closing_end),
            (7, 4, 7, 7)
        );
        assert_eq!(blocks("```")[0].marker_end, 3);
    }

    #[test]
    fn image_on_its_own_line() {
        let b = &blocks("![alt](Tasks/a.png)")[0];
        assert_eq!((b.kind, b.target.as_str()), (Block::Image, "Tasks/a.png"));
        for text in ["a ![](x.png)", "![](x.png) a", "![]()", "![a]b](c)"] {
            assert_eq!(blocks(text)[0].kind, Block::Paragraph, "{text}");
        }
    }

    #[test]
    fn inline_spans_follow_the_marker() {
        let d = parse_description("- a *b*");
        assert_eq!(d.inlines, [span(Inline::Emphasis, 4, 7, 1)]);
    }

    #[test]
    fn links() {
        let d = parse_description("see [the page](https://x.y/z) now");
        assert_eq!(
            d.links,
            [LinkSpan {
                start: 4,
                end: 29,
                text_start: 5,
                text_end: 13,
                target: "https://x.y/z".into()
            }]
        );
    }

    #[test]
    fn broken_links_stay_literal() {
        for text in [
            "[a]",
            "[a] (b)",
            "[](b)",
            "[a]()",
            "[a(b)",
            "![a](b) c",
            "`[a](b)`",
        ] {
            assert!(parse_description(text).links.is_empty(), "{text}");
        }
    }

    #[test]
    fn link_text_may_be_formatted() {
        let d = parse_description("[*a*](b)");
        assert_eq!(d.links.len(), 1);
        assert_eq!(d.inlines, [span(Inline::Emphasis, 1, 4, 1)]);
    }

    #[test]
    fn description_ranges_count_utf16_code_units() {
        let d = parse_description("😀\n- *😀* [é](u)");
        assert_eq!(d.blocks[1].start, 3);
        assert_eq!(d.blocks[1].marker_end, 5);
        assert_eq!(d.inlines, [span(Inline::Emphasis, 5, 9, 1)]);
        assert_eq!((d.links[0].start, d.links[0].end), (10, 16));
    }

    fn raw(text: &str, caret: u32) -> (u32, u32) {
        let r = raw_range(text, caret);
        (r.start, r.end)
    }

    #[test]
    fn raw_range_is_the_carets_line() {
        assert_eq!(raw("ab\ncd", 0), (0, 2));
        assert_eq!(raw("ab\ncd", 2), (0, 2));
        assert_eq!(raw("ab\ncd", 3), (3, 5));
        assert_eq!(raw("ab\ncd", 5), (3, 5));
        assert_eq!(raw("ab\n", 3), (3, 3));
        assert_eq!(raw("", 0), (0, 0));
        assert_eq!(raw("ab", 99), (0, 2));
        assert_eq!(raw("😀\nb", 3), (3, 4));
    }

    #[test]
    fn raw_range_is_the_whole_fence() {
        let text = "a\n```\nb\n\nc\n```\nd";
        assert_eq!(raw(text, 7), (2, 14));
        assert_eq!(raw(text, 2), (2, 14));
        assert_eq!(raw(text, 14), (2, 14));
        assert_eq!(raw(text, 1), (0, 1));
        assert_eq!(raw(text, 15), (15, 16));
    }

    /// The text after Return at `start..end`, with the caret as `|`.
    fn pressed(text: &str, start: u32, end: u32) -> String {
        apply(text, &return_at(text, start, end))
    }

    fn apply(text: &str, e: &TextEdit) -> String {
        let units: Vec<u16> = text.encode_utf16().collect();
        let mut out = String::from_utf16(&units[..e.start as usize]).unwrap();
        out += &e.replacement;
        out += &String::from_utf16(&units[e.end as usize..]).unwrap();
        out
    }

    #[test]
    fn return_continues_a_bullet() {
        let e = return_at("- a", 3, 3);
        assert_eq!(apply("- a", &e), "- a\n- ");
        assert_eq!((e.selection_start, e.selection_end), (6, 6));
    }

    #[test]
    fn return_continues_at_the_same_indentation() {
        assert_eq!(pressed("  - a", 5, 5), "  - a\n  - ");
    }

    #[test]
    fn return_counts_a_numbered_item_up() {
        assert_eq!(pressed("3. foo", 6, 6), "3. foo\n4. ");
        assert_eq!(pressed("9. a", 4, 4), "9. a\n10. ");
    }

    #[test]
    fn return_in_the_middle_of_an_item_splits_it() {
        assert_eq!(pressed("- ab", 3, 3), "- a\n- b");
    }

    #[test]
    fn return_on_an_empty_item_removes_its_marker() {
        let e = return_at("- a\n- ", 6, 6);
        assert_eq!(apply("- a\n- ", &e), "- a\n");
        assert_eq!(e.selection_start, 4);
        assert_eq!(pressed("1. a\n  2. ", 10, 10), "1. a\n");
    }

    #[test]
    fn return_elsewhere_is_a_plain_break() {
        let e = return_at("ab", 1, 1);
        assert_eq!((apply("ab", &e).as_str(), e.selection_start), ("a\nb", 2));
        assert_eq!(pressed("# - a", 5, 5), "# - a\n");
        assert_eq!(pressed("- a", 1, 1), "-\n a");
        assert_eq!(pressed("```\n- a\n```", 7, 7), "```\n- a\n\n```");
        assert_eq!(pressed("", 0, 0), "\n");
    }

    #[test]
    fn return_replaces_the_selection() {
        assert_eq!(pressed("- abc", 3, 4), "- a\n- c");
        assert_eq!(pressed("- ", 0, 2), "\n");
    }

    #[test]
    fn return_counts_utf16_units() {
        assert_eq!(pressed("- 😀", 4, 4), "- 😀\n- ");
    }

    fn shifted_by(text: &str, start: u32, end: u32, outdent: bool) -> Option<(String, u32, u32)> {
        shift_items(text, start, end, outdent)
            .map(|e| (apply(text, &e), e.selection_start, e.selection_end))
    }

    #[test]
    fn indent_adds_two_spaces_over_every_item_in_the_range() {
        let text = "- a\nx\n1. b\n- c";
        let (out, s, e) = shifted_by(text, 2, 11, false).unwrap();
        assert_eq!(out, "  - a\nx\n  1. b\n- c");
        assert_eq!((s, e), (4, 15));
    }

    #[test]
    fn indent_with_a_caret_moves_it_along() {
        let (out, s, e) = shifted_by("- a\n- b", 7, 7, false).unwrap();
        assert_eq!((out.as_str(), s, e), ("- a\n  - b", 9, 9));
    }

    #[test]
    fn a_range_ending_at_a_line_start_leaves_that_line() {
        let (out, ..) = shifted_by("- a\n- b", 0, 4, false).unwrap();
        assert_eq!(out, "  - a\n- b");
    }

    #[test]
    fn outdent_removes_up_to_two_spaces() {
        let (out, s, e) = shifted_by("    - a\n - b", 6, 12, true).unwrap();
        assert_eq!(out, "  - a\n- b");
        assert_eq!((s, e), (4, 9));
    }

    #[test]
    fn outdent_at_column_zero_does_nothing() {
        assert_eq!(shift_items("- a\n1. b", 0, 8, true), None);
    }

    #[test]
    fn shifting_ignores_lines_that_are_no_items() {
        assert_eq!(shift_items("a\n# b", 0, 5, false), None);
        assert_eq!(shift_items("```\n- a\n```", 0, 11, false), None);
    }

    fn toggled(text: &str, start: u32, end: u32, kind: Inline) -> (String, u32, u32) {
        let e = toggle_inline(text, start, end, kind);
        (apply(text, &e), e.selection_start, e.selection_end)
    }

    #[test]
    fn toggling_wraps_the_selection() {
        let wrapped = |kind| toggled("a b c", 2, 3, kind);
        assert_eq!(wrapped(Inline::Strong), ("a **b** c".into(), 4, 5));
        assert_eq!(wrapped(Inline::Emphasis), ("a *b* c".into(), 3, 4));
        assert_eq!(wrapped(Inline::Code), ("a `b` c".into(), 3, 4));
    }

    #[test]
    fn toggling_an_empty_selection_inserts_a_pair() {
        assert_eq!(toggled("ab", 1, 1, Inline::Strong), ("a****b".into(), 3, 3));
        assert_eq!(toggled("", 0, 0, Inline::Code), ("``".into(), 1, 1));
    }

    #[test]
    fn toggling_unwraps_when_the_markers_are_selected() {
        assert_eq!(
            toggled("a **b** c", 2, 7, Inline::Strong),
            ("a b c".into(), 2, 3)
        );
        assert_eq!(toggled("`b`", 0, 3, Inline::Code), ("b".into(), 0, 1));
    }

    #[test]
    fn toggling_unwraps_when_the_markers_are_just_outside() {
        assert_eq!(
            toggled("a **b** c", 4, 5, Inline::Strong),
            ("a b c".into(), 2, 3)
        );
        assert_eq!(toggled("*b*", 1, 2, Inline::Emphasis), ("b".into(), 0, 1));
    }

    #[test]
    fn toggling_a_selection_around_a_span_wraps_the_selection() {
        assert_eq!(
            toggled("a **b** c", 0, 9, Inline::Strong),
            ("**a **b** c**".into(), 2, 11)
        );
        assert_eq!(
            toggled("a **b** ", 2, 8, Inline::Strong),
            ("a b ".into(), 2, 4)
        );
    }

    #[test]
    fn toggling_unwraps_with_the_caret_in_the_span() {
        assert_eq!(toggled("*ab*", 2, 2, Inline::Emphasis), ("ab".into(), 1, 1));
    }

    #[test]
    fn emphasis_and_strong_are_not_confused() {
        assert_eq!(
            toggled("**b**", 2, 3, Inline::Emphasis),
            ("***b***".into(), 3, 4)
        );
        assert_eq!(
            toggled("*b*", 1, 2, Inline::Strong),
            ("***b***".into(), 3, 4)
        );
        assert_eq!(
            toggled("***b***", 3, 4, Inline::Emphasis),
            ("**b**".into(), 2, 3)
        );
        assert_eq!(
            toggled("***b***", 3, 4, Inline::Strong),
            ("*b*".into(), 1, 2)
        );
    }

    #[test]
    fn toggling_keeps_spaces_outside_the_markers() {
        assert_eq!(
            toggled("a b ", 2, 4, Inline::Emphasis),
            ("a *b* ".into(), 3, 4)
        );
        assert_eq!(
            toggled("a  b", 1, 3, Inline::Strong),
            ("a  ****b".into(), 5, 5)
        );
    }

    #[test]
    fn toggling_in_a_description_line_and_utf16() {
        assert_eq!(
            toggled("- 😀 x", 5, 6, Inline::Code),
            ("- 😀 `x`".into(), 6, 7)
        );
        assert_eq!(
            toggled("```\n*a*\n```", 5, 6, Inline::Emphasis),
            ("```\n**a**\n```".into(), 6, 7)
        );
    }
}
