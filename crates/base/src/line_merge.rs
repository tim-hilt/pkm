//! Three-way merge of texts by line (ADR-0032). Changes to different lines
//! are all kept; where both sides changed the same lines, the newer side wins.
//! Lines keep their terminators, so line endings survive the merge.

/// One of the two sides of a merge.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Local,
    Remote,
}

/// Replaces `base[start..end]` with `lines`; `start == end` is an insertion.
struct Hunk<'a> {
    start: usize,
    end: usize,
    lines: Vec<&'a str>,
}

impl Hunk<'_> {
    fn is_insertion(&self) -> bool {
        self.start == self.end
    }
}

/// Merges `local` and `remote`, both derived from `base`. `newer` wins the
/// lines both sides changed, whichever side is local.
pub fn merge(base: &str, local: &str, remote: &str, newer: Side) -> String {
    let base: Vec<&str> = base.split_inclusive('\n').collect();
    let mut hunks: Vec<(Side, Hunk)> = Vec::new();
    for (side, text) in [(Side::Local, local), (Side::Remote, remote)] {
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        hunks.extend(diff(&base, &lines).into_iter().map(|h| (side, h)));
    }
    // Insertions sort before an edit starting at the same line.
    hunks.sort_by_key(|(_, h)| (h.start, !h.is_insertion()));

    let mut out = String::new();
    let mut cursor = 0;
    let mut i = 0;
    while i < hunks.len() {
        let (side, hunk) = &hunks[i];
        let start = hunk.start;
        out.extend(base[cursor..start].iter().copied());
        if hunk.is_insertion() {
            // Both sides inserting at one spot keep both, the older first.
            let twin = hunks
                .get(i + 1)
                .filter(|(s, h)| s != side && h.is_insertion() && h.start == start);
            match twin {
                Some((_, other)) if other.lines != hunk.lines => {
                    let (older, newer) = if *side == newer {
                        (other, hunk)
                    } else {
                        (hunk, other)
                    };
                    out.extend(older.lines.iter().copied());
                    out.extend(newer.lines.iter().copied());
                    i += 2;
                }
                Some(_) => {
                    out.extend(hunk.lines.iter().copied());
                    i += 2;
                }
                None => {
                    out.extend(hunk.lines.iter().copied());
                    i += 1;
                }
            }
            cursor = start;
            continue;
        }
        // A cluster: hunks chained by overlap, resolved as one region.
        let mut end = hunk.end;
        let mut j = i + 1;
        while j < hunks.len() && hunks[j].1.start < end {
            end = end.max(hunks[j].1.end);
            j += 1;
        }
        let cluster = &hunks[i..j];
        let wins = if cluster.iter().any(|(s, _)| *s == newer) {
            newer
        } else {
            cluster[0].0
        };
        let mut at = start;
        for (_, h) in cluster.iter().filter(|(s, _)| *s == wins) {
            out.extend(base[at..h.start].iter().copied());
            out.extend(h.lines.iter().copied());
            at = h.end;
        }
        out.extend(base[at..end].iter().copied());
        cursor = end;
        i = j;
    }
    out.extend(base[cursor..].iter().copied());
    out
}

/// The hunks turning `old` into `new`, from a longest common subsequence of
/// the lines between their shared head and tail.
fn diff<'a>(old: &[&str], new: &[&'a str]) -> Vec<Hunk<'a>> {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..]
        .iter()
        .rev()
        .zip(new[head..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let (o, n) = (&old[head..old.len() - tail], &new[head..new.len() - tail]);

    // lcs[a][b]: common length of o[a..] and n[b..].
    let width = n.len() + 1;
    let mut lcs = vec![0u32; (o.len() + 1) * width];
    for a in (0..o.len()).rev() {
        for b in (0..n.len()).rev() {
            lcs[a * width + b] = if o[a] == n[b] {
                lcs[(a + 1) * width + b + 1] + 1
            } else {
                lcs[(a + 1) * width + b].max(lcs[a * width + b + 1])
            };
        }
    }

    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut a, mut b) = (0, 0);
    let mut open: Option<(usize, usize)> = None; // (old start, new start)
    while a < o.len() || b < n.len() {
        if a < o.len() && b < n.len() && o[a] == n[b] {
            if let Some((os, ns)) = open.take() {
                hunks.push(Hunk {
                    start: head + os,
                    end: head + a,
                    lines: n[ns..b].to_vec(),
                });
            }
            a += 1;
            b += 1;
            continue;
        }
        open.get_or_insert((a, b));
        if b == n.len() || (a < o.len() && lcs[(a + 1) * width + b] >= lcs[a * width + b + 1]) {
            a += 1;
        } else {
            b += 1;
        }
    }
    if let Some((os, ns)) = open {
        hunks.push(Hunk {
            start: head + os,
            end: head + a,
            lines: n[ns..b].to_vec(),
        });
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::*;
    use Side::{Local, Remote};

    #[test]
    fn edits_on_different_lines_are_all_kept() {
        let base = "a\nb\nc\nd\n";
        assert_eq!(
            merge(base, "A\nb\nc\nd\n", "a\nb\nc\nD\n", Local),
            "A\nb\nc\nD\n"
        );
        assert_eq!(
            merge(base, "a\nb\nc\nd\ne\n", "x\na\nb\nc\nd\n", Remote),
            "x\na\nb\nc\nd\ne\n"
        );
    }

    #[test]
    fn unchanged_side_takes_the_other() {
        assert_eq!(merge("a\nb\n", "a\nb\n", "a\nB\n", Local), "a\nB\n");
        assert_eq!(merge("a\nb\n", "a\nB\n", "a\nb\n", Remote), "a\nB\n");
    }

    #[test]
    fn same_lines_take_the_newer_side_whichever_is_local() {
        let base = "a\nb\nc\n";
        assert_eq!(merge(base, "a\nL\nc\n", "a\nR\nc\n", Local), "a\nL\nc\n");
        assert_eq!(merge(base, "a\nL\nc\n", "a\nR\nc\n", Remote), "a\nR\nc\n");
        // Swapping the sides and the newer side gives the same text.
        assert_eq!(merge(base, "a\nR\nc\n", "a\nL\nc\n", Local), "a\nR\nc\n");
    }

    #[test]
    fn overlapping_hunks_of_different_sizes_take_the_newer_whole() {
        let base = "a\nb\nc\nd\n";
        let local = "a\nX\nd\n"; // b, c -> X
        let remote = "a\nb\nY\nd\n"; // c -> Y
        assert_eq!(merge(base, local, remote, Local), "a\nX\nd\n");
        assert_eq!(merge(base, local, remote, Remote), "a\nb\nY\nd\n");
    }

    #[test]
    fn insertions_at_the_same_spot_are_both_kept_older_first() {
        let base = "a\nb\n";
        assert_eq!(
            merge(base, "a\nL\nb\n", "a\nR\nb\n", Remote),
            "a\nL\nR\nb\n"
        );
        assert_eq!(merge(base, "a\nL\nb\n", "a\nR\nb\n", Local), "a\nR\nL\nb\n");
        assert_eq!(merge(base, "a\nS\nb\n", "a\nS\nb\n", Local), "a\nS\nb\n");
    }

    #[test]
    fn insertion_next_to_an_edit_is_kept() {
        assert_eq!(merge("a\nb\n", "a\nN\nb\n", "a\nB\n", Remote), "a\nN\nB\n");
    }

    #[test]
    fn deletion_against_an_edit_takes_the_newer_side() {
        let base = "a\nb\nc\n";
        assert_eq!(merge(base, "a\nc\n", "a\nB\nc\n", Local), "a\nc\n");
        assert_eq!(merge(base, "a\nc\n", "a\nB\nc\n", Remote), "a\nB\nc\n");
    }

    #[test]
    fn deletion_on_one_side_and_edit_elsewhere_are_both_kept() {
        assert_eq!(merge("a\nb\nc\n", "b\nc\n", "a\nb\nC\n", Local), "b\nC\n");
    }

    #[test]
    fn empty_base_keeps_both_sides() {
        assert_eq!(merge("", "L\n", "R\n", Remote), "L\nR\n");
        assert_eq!(merge("", "", "R\n", Local), "R\n");
        assert_eq!(merge("", "", "", Local), "");
    }

    #[test]
    fn missing_trailing_newline_is_part_of_the_last_line() {
        assert_eq!(merge("a\nb", "A\nb", "a\nb", Local), "A\nb");
        assert_eq!(merge("a\nb", "a\nb\n", "A\nb", Local), "A\nb\n");
        assert_eq!(merge("a\nb", "a\nb\n", "A\nb", Remote), "A\nb\n");
        assert_eq!(merge("a\nb", "a\nb\n", "a\nb", Local), "a\nb\n");
    }

    #[test]
    fn line_endings_are_preserved() {
        let base = "a\r\nb\r\nc\r\n";
        assert_eq!(
            merge(base, "A\r\nb\r\nc\r\n", "a\r\nb\r\nC\r\n", Local),
            "A\r\nb\r\nC\r\n"
        );
        assert_eq!(
            merge("a\nb\n", "a\nb\nc\r\n", "A\nb\n", Local),
            "A\nb\nc\r\n"
        );
    }
}
