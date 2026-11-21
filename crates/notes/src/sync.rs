//! Merging what two Devices have kept of the same Note (ADR-0021, ADR-0032).
//! The Sync engine (`pkm-cloud`) asks for these bytes; the rules live here,
//! beside the file format they decide about.
//!
//! Three-way against the file at the last Sync. The Title (like any other
//! property) takes the side that changed it since the base, or the newer
//! `updated` when both did. The Body merges by line, the newer side winning
//! the lines both changed. Without a base the newer side wins whole. Bytes
//! that are no Note are no Item: `merge` and `same_item` say none.
//!
//! Image files (`store::images`) are Items too, but are transferred and never
//! merged: an Image's file never changes after it is added.

use pkm_base::front_matter::Document;
use pkm_base::images::{is_image, is_image_file};
use pkm_base::line_merge::{self, Side};
use std::cmp::Ordering;

const UPDATED: &str = "updated";

/// What `local` and `remote` merge to, given the Note's bytes at the last Sync
/// (none if there was none); none if either isn't a Note or they differ in id.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    if local == remote {
        return Some(local.to_vec());
    }
    let (left, right) = (parse(local)?, parse(remote)?);
    if left.scalar("id") != right.scalar("id") {
        return None;
    }
    // An unreadable base is as good as none.
    let base = base.and_then(parse);
    // Both Devices must pick the same winner, whichever side they call local.
    let newer = match updated(&left).cmp(updated(&right)) {
        Ordering::Equal if local.max(remote) == local => Side::Local,
        Ordering::Greater => Side::Local,
        _ => Side::Remote,
    };
    let (winner, loser) = match newer {
        Side::Local => (&left, &right),
        Side::Remote => (&right, &left),
    };
    let mut merged = winner.clone();
    let keys: Vec<&str> = winner.keys().chain(loser.keys()).collect();
    for key in keys.into_iter().filter(|key| *key != UPDATED) {
        let base = base.as_ref().map(|b| b.get(key));
        // The winner's, unless only the loser changed it since the base.
        let value = if base == Some(winner.get(key)) {
            loser.get(key)
        } else {
            winner.get(key)
        };
        match value {
            Some(value) => merged.set(key, value.clone()),
            None => merged.remove(key),
        }
    }
    merged.body = match &base {
        Some(base) => line_merge::merge(&base.body, &left.body, &right.body, newer),
        None => winner.body.clone(),
    };
    Some(merged.render().into_bytes())
}

/// Whether a file of this name is a Note's or an Image's: not hidden.
pub fn accepts(file: &str) -> bool {
    !file.starts_with('.') && (file.ends_with(".md") || is_image_file(file))
}

/// Whether `bytes` are a Note or an Image.
pub fn is_item(bytes: &[u8]) -> bool {
    is_image(bytes) || parse(bytes).is_some()
}

/// Whether two Notes are one Note: they share an id; two Images, if they are
/// the same bytes. None if either isn't a Note or an Image, or they differ in
/// kind.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    match (is_image(a), is_image(b)) {
        (true, true) => Some(a == b),
        (false, false) => Some(parse(a)?.scalar("id") == parse(b)?.scalar("id")),
        _ => None,
    }
}

/// The Note a file holds: a document with an `id`.
fn parse(bytes: &[u8]) -> Option<Document> {
    let file = Document::parse(std::str::from_utf8(bytes).ok()?)?;
    file.scalar("id").is_some().then_some(file)
}

fn updated(file: &Document) -> &str {
    file.scalar(UPDATED).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: &str = "2024-01-01T00:00:00Z";
    const T1: &str = "2024-01-02T00:00:00Z";
    const T2: &str = "2024-01-03T00:00:00Z";

    fn note(title: &str, body: &str, updated: &str) -> Vec<u8> {
        format!("---\nid: aaaaaaaaaaaa\ntitle: {title}\nupdated: {updated}\n---\n{body}")
            .into_bytes()
    }

    fn merged(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Document {
        let bytes = merge(base, local, remote).unwrap();
        // Whichever Device merges, the result is the same.
        assert_eq!(merge(base, remote, local).unwrap(), bytes);
        Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap()
    }

    #[test]
    fn edits_to_different_body_lines_are_both_kept() {
        let base = note("Plan", "one\ntwo\nthree\n", T0);
        let local = note("Plan", "ONE\ntwo\nthree\n", T1);
        let remote = note("Plan", "one\ntwo\nTHREE\n", T2);
        let file = merged(Some(&base), &local, &remote);
        assert_eq!(file.body, "ONE\ntwo\nTHREE\n");
        assert_eq!(file.scalar("updated"), Some(T2));
    }

    #[test]
    fn the_same_body_lines_edited_on_both_take_the_newer_side() {
        let base = note("Plan", "one\ntwo\n", T0);
        let older = note("Plan", "uno\ntwo\n", T1);
        let newer = note("Plan", "eins\ntwo\n", T2);
        assert_eq!(merged(Some(&base), &older, &newer).body, "eins\ntwo\n");
    }

    #[test]
    fn the_title_takes_the_side_that_changed_it() {
        let base = note("Plan", "one\n", T0);
        let renamed = note("Roadmap", "one\n", T1);
        let edited = note("Plan", "two\n", T2);
        let file = merged(Some(&base), &renamed, &edited);
        assert_eq!(file.scalar("title"), Some("Roadmap"));
        assert_eq!(file.body, "two\n");
    }

    #[test]
    fn titles_changed_on_both_take_the_newer() {
        let base = note("Plan", "", T0);
        let older = note("Roadmap", "", T1);
        let newer = note("Agenda", "", T2);
        assert_eq!(
            merged(Some(&base), &older, &newer).scalar("title"),
            Some("Agenda")
        );
    }

    #[test]
    fn without_a_base_the_newer_side_wins_whole() {
        let older = note("Roadmap", "one\n", T1);
        let newer = note("Agenda", "two\n", T2);
        let file = merged(None, &older, &newer);
        assert_eq!(file.scalar("title"), Some("Agenda"));
        assert_eq!(file.body, "two\n");
    }

    #[test]
    fn files_that_are_no_notes_are_no_items() {
        let good = note("Plan", "", T0);
        for bad in [&b"just text"[..], b"---\ntitle: x\n---\n", b"\xff\xfe"] {
            assert!(!is_item(bad));
            assert_eq!(merge(None, &good, bad), None);
            assert_eq!(same_item(&good, bad), None);
        }
        assert!(is_item(&good));
    }

    #[test]
    fn notes_of_other_ids_neither_merge_nor_are_the_same() {
        let a = note("Plan", "", T0);
        let b = String::from_utf8(a.clone())
            .unwrap()
            .replace("aaaaaaaaaaaa", "bbbbbbbbbbbb");
        assert_eq!(merge(None, &a, b.as_bytes()), None);
        assert_eq!(same_item(&a, b.as_bytes()), Some(false));
        assert_eq!(same_item(&a, &a), Some(true));
    }

    #[test]
    fn images_are_items_that_are_the_same_only_by_their_bytes() {
        let png = b"\x89PNG\r\n\x1a\n a";
        let note = note("Plan", "", T0);
        assert!(is_item(png));
        assert_eq!(same_item(png, png), Some(true));
        assert_eq!(same_item(png, b"\x89PNG\r\n\x1a\n b"), Some(false));
        assert_eq!(same_item(png, &note), None);
    }

    #[test]
    fn only_visible_markdown_and_image_files_are_accepted() {
        assert!(accepts("aaaaaaaaaaaa.md") && accepts("aaaaaaaaaaaa.png"));
        for file in [
            ".aaaaaaaaaaaa.md",
            ".aaaaaaaaaaaa.png",
            "photo.png",
            "a.txt",
        ] {
            assert!(!accepts(file), "{file}");
        }
    }
}
