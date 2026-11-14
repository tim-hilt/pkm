//! Quick Add: a Title typed with Shortcuts in it (Tasks glossary). Owns every
//! Shortcut rule, so clients only draw the field and forward edits; no I/O.
//!
//! Positions and ranges are in UTF-16 code units, which is how Swift's text
//! views and JavaScript count. Only Priority Shortcuts exist so far, so
//! `tags` is always empty.

/// What a Shortcut sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShortcutMeaning {
    /// `p1` to `p4`.
    Priority(u8),
}

/// A Shortcut in the text, at `start..end` (UTF-16 code units).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuickShortcut {
    pub start: u32,
    pub end: u32,
    pub meaning: ShortcutMeaning,
}

/// The text of one Title field, with the cursor in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickAdd {
    text: Vec<u16>,
    cursor: usize,
}

impl QuickAdd {
    /// An empty text, for a new Task.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces `start..end` with `with` (typing, deleting, pasting) and moves
    /// the cursor behind it. Line breaks become spaces; a range beyond the
    /// text is clamped.
    pub fn replace(&mut self, start: u32, end: u32, with: &str) {
        let end = (end as usize).min(self.text.len());
        let start = (start as usize).min(end);
        let with: Vec<u16> = single_line(with).encode_utf16().collect();
        self.cursor = start + with.len();
        self.text.splice(start..end, with);
    }

    /// Puts the cursor at `at`, clamped to the text.
    pub fn move_cursor(&mut self, at: u32) {
        self.cursor = (at as usize).min(self.text.len());
    }

    /// The text as typed, Shortcuts included.
    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// The Shortcuts to draw highlighted: all but the one the cursor is in or
    /// at the end of, which the user may still be typing.
    pub fn highlights(&self) -> Vec<QuickShortcut> {
        let mut shortcuts = self.shortcuts();
        shortcuts.retain(|s| !(s.start as usize..=s.end as usize).contains(&self.cursor));
        shortcuts
    }

    /// The text without any Shortcut (even one under the cursor), whitespace
    /// collapsed to single spaces and trimmed. Empty if nothing else is typed.
    pub fn title(&self) -> String {
        let mut rest = String::new();
        let mut from = 0;
        for s in self.shortcuts() {
            rest.push_str(&String::from_utf16_lossy(
                &self.text[from..s.start as usize],
            ));
            rest.push(' ');
            from = s.end as usize;
        }
        rest.push_str(&String::from_utf16_lossy(&self.text[from..]));
        rest.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    /// The Priority to apply, if a Shortcut sets one; the last one wins.
    pub fn priority(&self) -> Option<u8> {
        let ShortcutMeaning::Priority(p) = self.shortcuts().first()?.meaning;
        Some(p)
    }

    /// The Tags to add.
    pub fn tags(&self) -> Vec<String> {
        Vec::new()
    }

    /// Every Shortcut in the text, in order, under the cursor or not. Of
    /// several Priority Shortcuts only the last counts; earlier ones are plain.
    fn shortcuts(&self) -> Vec<QuickShortcut> {
        let last = words(&self.text)
            .filter_map(|(start, end)| {
                priority_of(&self.text[start..end]).map(|p| QuickShortcut {
                    start: start as u32,
                    end: end as u32,
                    meaning: ShortcutMeaning::Priority(p),
                })
            })
            .last();
        last.into_iter().collect()
    }
}

/// `text` with every line break (`\r\n` counting as one) turned into a space.
fn single_line(text: &str) -> String {
    text.replace("\r\n", " ").replace(['\r', '\n'], " ")
}

fn is_space(unit: u16) -> bool {
    char::from_u32(u32::from(unit)).is_some_and(char::is_whitespace)
}

/// The ranges of the whitespace-separated words in `text`.
fn words(text: &[u16]) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        let start = at + text[at..].iter().position(|&u| !is_space(u))?;
        let end = text[start..]
            .iter()
            .position(|&u| is_space(u))
            .map_or(text.len(), |n| start + n);
        at = end;
        Some((start, end))
    })
}

/// The Priority a word sets: `p1` to `p4`, lowercase.
fn priority_of(word: &[u16]) -> Option<u8> {
    match word {
        [p, digit] if *p == u16::from(b'p') => match u8::try_from(*digit).ok()? {
            d @ b'1'..=b'4' => Some(d - b'0'),
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Types `text` at the end, as if pasted, with the cursor behind it.
    fn typed(text: &str) -> QuickAdd {
        let mut quick = QuickAdd::new();
        quick.replace(0, 0, text);
        quick
    }

    /// `typed` with the cursor moved away from the Shortcuts, to the start.
    fn typed_cursor_at_start(text: &str) -> QuickAdd {
        let mut quick = typed(text);
        quick.move_cursor(0);
        quick
    }

    fn priority(start: u32, end: u32, p: u8) -> QuickShortcut {
        QuickShortcut {
            start,
            end,
            meaning: ShortcutMeaning::Priority(p),
        }
    }

    #[test]
    fn starts_empty() {
        let quick = QuickAdd::new();
        assert_eq!(quick.text(), "");
        assert_eq!(quick.title(), "");
        assert_eq!(quick.priority(), None);
        assert!(quick.highlights().is_empty());
        assert!(quick.tags().is_empty());
    }

    #[test]
    fn a_priority_shortcut_sets_the_priority_and_leaves_the_title() {
        let quick = typed_cursor_at_start("Fix login p1");
        assert_eq!(quick.priority(), Some(1));
        assert_eq!(quick.title(), "Fix login");
        assert_eq!(quick.highlights(), [priority(10, 12, 1)]);
        assert_eq!(quick.text(), "Fix login p1");
    }

    #[test]
    fn every_priority_from_p1_to_p4_counts() {
        for p in 1..=4 {
            assert_eq!(typed(&format!("x p{p} ")).priority(), Some(p));
        }
    }

    #[test]
    fn only_whole_lowercase_words_p1_to_p4_match() {
        for text in [
            "P1", "p5", "p0", "p", "top1", "p1x", "p12", "p1.", "(p1)", "p-1",
        ] {
            let quick = typed_cursor_at_start(&format!("a {text}"));
            assert_eq!(quick.priority(), None, "{text}");
            assert!(quick.highlights().is_empty(), "{text}");
            assert_eq!(quick.title(), format!("a {text}"), "{text}");
        }
    }

    #[test]
    fn a_shortcut_may_come_first_or_between_words() {
        assert_eq!(typed_cursor_at_start("p2 Fix login").title(), "Fix login");
        assert_eq!(typed_cursor_at_start("Fix p3 login").title(), "Fix login");
    }

    #[test]
    fn the_last_priority_wins_and_earlier_ones_stay_in_the_title() {
        let quick = typed_cursor_at_start("p1 Fix p2 login p3");
        assert_eq!(quick.priority(), Some(3));
        assert_eq!(quick.highlights(), [priority(16, 18, 3)]);
        assert_eq!(quick.title(), "p1 Fix p2 login");
    }

    #[test]
    fn the_title_collapses_whitespace_and_is_trimmed() {
        assert_eq!(
            typed_cursor_at_start("  Call   p1  about \t rent ").title(),
            "Call about rent"
        );
    }

    #[test]
    fn only_shortcuts_leave_an_empty_title() {
        assert_eq!(typed("p1").title(), "");
        assert_eq!(typed(" p2 ").title(), "");
    }

    #[test]
    fn line_breaks_become_spaces() {
        let quick = typed("a\nb\r\nc\rd");
        assert_eq!(quick.text(), "a b c d");
        assert_eq!(quick.title(), "a b c d");
    }

    #[test]
    fn a_line_break_ends_a_word() {
        let quick = typed_cursor_at_start("Fix\np1");
        assert_eq!(quick.priority(), Some(1));
        assert_eq!(quick.text(), "Fix p1");
    }

    #[test]
    fn the_shortcut_under_the_cursor_is_not_highlighted_but_counts() {
        let mut quick = typed("Fix login p1");
        // The cursor is behind `p1`.
        assert!(quick.highlights().is_empty());
        assert_eq!(quick.priority(), Some(1));
        assert_eq!(quick.title(), "Fix login");

        quick.move_cursor(11); // inside the word
        assert!(quick.highlights().is_empty());
        quick.move_cursor(10); // just before it
        assert!(quick.highlights().is_empty());

        quick.move_cursor(9); // in the space before it
        assert_eq!(quick.highlights(), [priority(10, 12, 1)]);
    }

    #[test]
    fn typing_a_space_after_a_shortcut_highlights_it() {
        let mut quick = typed("Fix p2");
        assert!(quick.highlights().is_empty());
        quick.replace(6, 6, " ");
        assert_eq!(quick.highlights(), [priority(4, 6, 2)]);
    }

    #[test]
    fn typing_builds_a_word_one_unit_at_a_time() {
        let mut quick = QuickAdd::new();
        quick.replace(0, 0, "p");
        assert_eq!(quick.priority(), None);
        quick.replace(1, 1, "3");
        assert_eq!(quick.priority(), Some(3));
        quick.replace(1, 2, "9");
        assert_eq!(quick.priority(), None);
    }

    #[test]
    fn replacing_a_range_moves_the_cursor_behind_the_new_text() {
        let mut quick = typed("Fix login");
        quick.replace(4, 9, "p4 ");
        assert_eq!(quick.text(), "Fix p4 ");
        assert_eq!(quick.highlights(), [priority(4, 6, 4)]);
    }

    #[test]
    fn positions_count_utf16_code_units() {
        // 🚀 is two code units, é one.
        let quick = typed_cursor_at_start("🚀 é p1");
        assert_eq!(quick.highlights(), [priority(5, 7, 1)]);
        assert_eq!(quick.title(), "🚀 é");
    }

    #[test]
    fn editing_after_non_ascii_text_uses_utf16_ranges() {
        let mut quick = typed("🚀 go p2");
        quick.replace(6, 8, "p1"); // `p2` after "🚀 go "
        assert_eq!(quick.text(), "🚀 go p1");
        assert_eq!(quick.priority(), Some(1));
    }

    #[test]
    fn out_of_range_positions_are_clamped() {
        let mut quick = typed("ab");
        quick.replace(5, 9, "c");
        assert_eq!(quick.text(), "abc");
        quick.move_cursor(99);
        assert_eq!(quick.highlights(), []);
    }
}
