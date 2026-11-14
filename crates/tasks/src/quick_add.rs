//! Quick Add: a Title typed with Shortcuts in it (Tasks glossary). Owns every
//! Shortcut rule, so clients only draw the field and forward edits; no I/O.
//!
//! Positions and ranges are in UTF-16 code units, which is how Swift's text
//! views and JavaScript count.

/// What a Shortcut sets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShortcutMeaning {
    /// `p1` to `p4`.
    Priority(u8),
    /// `@tag`, holding the Tag without the `@`.
    Tag(String),
}

/// A Shortcut in the text, at `start..end` (UTF-16 code units).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickShortcut {
    pub start: u32,
    pub end: u32,
    pub meaning: ShortcutMeaning,
}

/// What to suggest while the cursor is in an `@` word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagQuery {
    /// The word after its `@`; empty for a lone `@`.
    pub query: String,
    /// Tags typed as Shortcuts in other words, which need no suggesting again.
    pub leave_out: Vec<String>,
}

/// The text of one Title field, with the cursor in it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuickAdd {
    text: Vec<u16>,
    cursor: usize,
    /// Ranges of Shortcuts the user Ignored; each is still a whole word.
    ignored: Vec<(usize, usize)>,
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
        let grown = with.len() as isize - (end - start) as isize;
        self.text.splice(start..end, with);
        // An edit before an Ignore shifts it, one touching it clears it.
        let mut ignored = std::mem::take(&mut self.ignored);
        ignored.retain_mut(|(s, e)| {
            if end <= *s {
                *s = s.wrapping_add_signed(grown);
                *e = e.wrapping_add_signed(grown);
                true
            } else {
                start >= *e
            }
        });
        // An edit right behind one grows its word, which is then new text.
        let text = &self.text;
        ignored.retain(|range| words(text).any(|word| word == *range));
        self.ignored = ignored;
    }

    /// Ignores the Shortcut containing `at` (UTF-16 position), turning it back
    /// into plain text of the Title. Does nothing if `at` is in no Shortcut.
    pub fn ignore(&mut self, at: u32) {
        let at = at as usize;
        if let Some(s) = self
            .shortcuts()
            .into_iter()
            .find(|s| (s.start as usize..s.end as usize).contains(&at))
        {
            self.ignored.push((s.start as usize, s.end as usize));
        }
    }

    /// Puts the cursor at `at`, clamped to the text.
    pub fn move_cursor(&mut self, at: u32) {
        self.cursor = (at as usize).min(self.text.len());
    }

    /// Where the cursor is (UTF-16 position), which `replace` and `pick_tag`
    /// move and a client must follow.
    pub fn cursor(&self) -> u32 {
        self.cursor as u32
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
        self.shortcuts().iter().find_map(|s| match s.meaning {
            ShortcutMeaning::Priority(p) => Some(p),
            ShortcutMeaning::Tag(_) => None,
        })
    }

    /// The Tags to add: every Tag Shortcut, a repeated one once, in order.
    pub fn tags(&self) -> Vec<String> {
        let mut tags = Vec::new();
        for s in self.shortcuts() {
            if let ShortcutMeaning::Tag(tag) = s.meaning
                && !tags.contains(&tag)
            {
                tags.push(tag);
            }
        }
        tags
    }

    /// What to suggest, if the cursor is in or at the end of an `@` word.
    pub fn tag_query(&self) -> Option<TagQuery> {
        let (start, end) = self.tag_word()?;
        let others = self
            .shortcuts()
            .into_iter()
            .filter_map(|s| match s.meaning {
                ShortcutMeaning::Tag(tag) if (s.start as usize, s.end as usize) != (start, end) => {
                    Some(tag)
                }
                _ => None,
            });
        Some(TagQuery {
            query: String::from_utf16_lossy(&self.text[start + 1..end]),
            leave_out: others.collect(),
        })
    }

    /// Completes the `@` word under the cursor to `@tag ` and puts the cursor
    /// behind the space. Does nothing outside an `@` word.
    pub fn pick_tag(&mut self, tag: &str) {
        if let Some((start, end)) = self.tag_word() {
            self.replace(start as u32, end as u32, &format!("@{tag} "));
        }
    }

    /// The range of the `@` word the cursor is in or at the end of.
    fn tag_word(&self) -> Option<(usize, usize)> {
        words(&self.text).find(|&(start, end)| {
            self.text[start] == u16::from(b'@') && (start..=end).contains(&self.cursor)
        })
    }

    /// Every Shortcut in the text, in order, under the cursor or not. Ignored
    /// ones are plain text. Of several Priority Shortcuts only the last counts;
    /// earlier ones are plain.
    fn shortcuts(&self) -> Vec<QuickShortcut> {
        let mut found: Vec<QuickShortcut> = words(&self.text)
            .filter(|word| !self.ignored.contains(word))
            .filter_map(|(start, end)| {
                meaning_of(&self.text[start..end]).map(|meaning| QuickShortcut {
                    start: start as u32,
                    end: end as u32,
                    meaning,
                })
            })
            .collect();
        let is_priority = |s: &QuickShortcut| matches!(s.meaning, ShortcutMeaning::Priority(_));
        if let Some(last) = found.iter().rposition(is_priority) {
            let mut at = 0;
            found.retain(|s| {
                at += 1;
                !is_priority(s) || at - 1 == last
            });
        }
        found
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

/// What a word sets, if it is a Shortcut.
fn meaning_of(word: &[u16]) -> Option<ShortcutMeaning> {
    if let Some(p) = priority_of(word) {
        return Some(ShortcutMeaning::Priority(p));
    }
    let (at, tag) = word.split_first()?;
    if *at != u16::from(b'@') {
        return None;
    }
    let tag = String::from_utf16(tag).ok()?;
    pkm_base::tag::is_valid(&tag).then_some(ShortcutMeaning::Tag(tag))
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

    fn tag(start: u32, end: u32, name: &str) -> QuickShortcut {
        QuickShortcut {
            start,
            end,
            meaning: ShortcutMeaning::Tag(name.to_owned()),
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

    #[test]
    fn a_tag_shortcut_adds_the_tag_and_leaves_the_title() {
        let quick = typed_cursor_at_start("Fix login @bug");
        assert_eq!(quick.tags(), ["bug"]);
        assert_eq!(quick.title(), "Fix login");
        assert_eq!(quick.highlights(), [tag(10, 14, "bug")]);
    }

    #[test]
    fn a_tag_shortcut_runs_to_the_next_whitespace() {
        let quick = typed_cursor_at_start("@work/project-x go");
        assert_eq!(quick.tags(), ["work/project-x"]);
        assert_eq!(quick.title(), "go");
    }

    #[test]
    fn only_an_at_at_the_start_of_a_valid_tag_word_matches() {
        for text in [
            "me@mail.com",
            "@Work",
            "@a//b",
            "@",
            "@a_b",
            "@/a",
            "a@b",
            "@@a",
        ] {
            let quick = typed_cursor_at_start(&format!("x {text}"));
            assert!(quick.tags().is_empty(), "{text}");
            assert!(quick.highlights().is_empty(), "{text}");
            assert_eq!(quick.title(), format!("x {text}"), "{text}");
        }
    }

    #[test]
    fn several_tag_shortcuts_all_apply() {
        let quick = typed_cursor_at_start("@a x @b p2 @c");
        assert_eq!(quick.tags(), ["a", "b", "c"]);
        assert_eq!(quick.priority(), Some(2));
        assert_eq!(quick.title(), "x");
    }

    #[test]
    fn a_repeated_tag_is_added_once_and_both_stay_highlighted() {
        let quick = typed_cursor_at_start(" @a x @a");
        assert_eq!(quick.tags(), ["a"]);
        assert_eq!(quick.highlights(), [tag(1, 3, "a"), tag(6, 8, "a")]);
    }

    #[test]
    fn a_losing_priority_does_not_hide_tags_between() {
        let quick = typed_cursor_at_start("p1 @a p2");
        assert_eq!(quick.highlights(), [tag(3, 5, "a"), priority(6, 8, 2)]);
        assert_eq!(quick.title(), "p1");
    }

    #[test]
    fn a_tag_under_the_cursor_is_not_highlighted_but_counts() {
        let quick = typed("Fix @bug");
        assert!(quick.highlights().is_empty());
        assert_eq!(quick.tags(), ["bug"]);
        assert_eq!(quick.title(), "Fix");
    }

    #[test]
    fn tag_positions_count_utf16_code_units() {
        let quick = typed_cursor_at_start("🚀 @a");
        assert_eq!(quick.highlights(), [tag(3, 5, "a")]);
    }

    #[test]
    fn no_query_outside_an_at_word() {
        assert_eq!(typed("Fix login").tag_query(), None);
        assert_eq!(typed("a@b").tag_query(), None);
        let mut quick = typed("Fix @bug now");
        quick.move_cursor(3); // in the space before the word
        assert_eq!(quick.tag_query(), None);
        quick.move_cursor(12); // behind `now`
        assert_eq!(quick.tag_query(), None);
    }

    #[test]
    fn a_lone_at_queries_everything() {
        let query = typed("Fix @").tag_query().unwrap();
        assert_eq!((query.query.as_str(), query.leave_out.len()), ("", 0));
    }

    #[test]
    fn the_query_is_the_word_after_the_at_wherever_the_cursor_is_in_it() {
        let mut quick = typed("Fix @bu");
        assert_eq!(quick.tag_query().unwrap().query, "bu");
        quick.move_cursor(5); // just behind the `@`
        assert_eq!(quick.tag_query().unwrap().query, "bu");
        quick.move_cursor(4); // just before the `@`
        assert_eq!(quick.tag_query().unwrap().query, "bu");
    }

    #[test]
    fn an_invalid_half_typed_word_is_still_queried() {
        assert_eq!(typed("@Wo").tag_query().unwrap().query, "Wo");
    }

    #[test]
    fn the_query_leaves_out_tags_typed_in_other_words_but_not_its_own() {
        let mut quick = typed("@a @b @c");
        quick.move_cursor(5); // in `@b`
        let query = quick.tag_query().unwrap();
        assert_eq!(query.query, "b");
        assert_eq!(query.leave_out, ["a", "c"]);
    }

    #[test]
    fn picking_completes_the_word_and_moves_the_cursor_behind_the_space() {
        let mut quick = typed("Fix @bu now");
        quick.move_cursor(7);
        quick.pick_tag("bug/ui");
        assert_eq!(quick.text(), "Fix @bug/ui  now");
        assert_eq!(quick.cursor(), 12);
    }

    #[test]
    fn a_picked_tag_applies_and_is_highlighted_once_the_cursor_leaves() {
        let mut quick = typed("Fix @b");
        quick.pick_tag("bug");
        assert_eq!(quick.text(), "Fix @bug ");
        assert_eq!(quick.tags(), ["bug"]);
        assert_eq!(quick.title(), "Fix");
        assert_eq!(quick.tag_query(), None); // the cursor is behind the space
        assert_eq!(quick.highlights(), [tag(4, 8, "bug")]);
    }

    #[test]
    fn picking_outside_an_at_word_changes_nothing() {
        let mut quick = typed("Fix");
        quick.pick_tag("bug");
        assert_eq!(quick.text(), "Fix");
    }

    #[test]
    fn an_ignored_shortcut_stays_in_the_title_and_sets_nothing() {
        let mut quick = typed_cursor_at_start("Fix p1 @bug");
        quick.ignore(5);
        quick.ignore(8);
        assert_eq!(quick.priority(), None);
        assert!(quick.tags().is_empty());
        assert!(quick.highlights().is_empty());
        assert_eq!(quick.title(), "Fix p1 @bug");
    }

    #[test]
    fn ignoring_outside_a_shortcut_changes_nothing() {
        let mut quick = typed_cursor_at_start("Fix p1");
        quick.ignore(1);
        quick.ignore(6); // just behind `p1`
        assert_eq!(quick.priority(), Some(1));
    }

    #[test]
    fn an_ignored_last_priority_lets_the_one_before_win() {
        let mut quick = typed_cursor_at_start("p1 x p2");
        quick.ignore(6);
        assert_eq!(quick.priority(), Some(1));
        assert_eq!(quick.title(), "x p2");
    }

    #[test]
    fn an_ignore_holds_while_typing_elsewhere() {
        let mut quick = typed("Fix p1 now");
        quick.ignore(5);
        quick.replace(0, 0, "Please "); // before: shifts
        quick.replace(quick.text().len() as u32, quick.text().len() as u32, "!"); // after
        quick.replace(7, 7, "a "); // right before the word, now at 7..
        assert_eq!(quick.text(), "Please a Fix p1 now!");
        assert_eq!(quick.priority(), None);
        quick.replace(0, 0, "x");
        assert_eq!(quick.priority(), None);
    }

    #[test]
    fn editing_inside_an_ignored_shortcut_clears_the_ignore() {
        let mut quick = typed("Fix p1 now");
        quick.ignore(5);
        quick.replace(5, 6, "2"); // `p2`, typed inside the Ignore
        assert_eq!(quick.priority(), Some(2));
    }

    #[test]
    fn deleting_and_retyping_an_ignored_shortcut_makes_it_a_shortcut_again() {
        let mut quick = typed("Fix p1 now");
        quick.ignore(5);
        quick.replace(4, 6, "");
        quick.replace(4, 4, "p1");
        assert_eq!(quick.priority(), Some(1));
    }

    #[test]
    fn an_edit_that_breaks_the_word_clears_the_ignore_even_if_undone_by_hand() {
        let mut quick = typed("Fix p1 now");
        quick.ignore(5);
        quick.replace(6, 6, "x"); // `p1x`
        quick.replace(6, 7, ""); // `p1` again
        assert_eq!(quick.priority(), Some(1));
    }

    #[test]
    fn picking_after_non_ascii_text_uses_utf16_positions() {
        let mut quick = typed("🚀 @b");
        quick.pick_tag("bug");
        assert_eq!(quick.text(), "🚀 @bug ");
        assert_eq!(quick.cursor(), 8);
    }
}
