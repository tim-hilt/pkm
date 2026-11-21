//! Merging what two Devices have kept of the same Bookmark: its `.md` file and
//! its state file (Bookmarks ADR-0002). The Sync engine (`pkm-cloud`) asks for
//! these bytes; the rules live here, beside the files they decide about.
//!
//! The file merges three-way against the file at the last Sync: a property
//! changed on one side takes that side, and one changed on both takes the side
//! with the newer `updated`. The Article is never edited, so the bodies are
//! the same, except that Drop Article wins over a body on the other side. The
//! state merges as Books' does, per Highlight and Comment, with the latest
//! reading. Bytes that are no Bookmark, or no state, are no Item: the merges
//! and `same_item` say none, so Sync leaves them as they are.
//!
//! Images are Items too, but are transferred and never merged: an image's file
//! never changes after it is kept.

use crate::state::State;
use pkm_base::front_matter::Document;
use pkm_base::images::{is_image, is_image_file};
use std::cmp::Ordering;

/// When a Bookmark's details last changed, UTC ISO 8601; absent in a file
/// that never changed after it was Saved.
pub(crate) const UPDATED: &str = "updated";

/// What `local` and `remote` merge to, given the file's bytes at the last Sync
/// (none if there was none); none if either isn't a Bookmark or they differ in
/// id. Two images merge only if they are the same bytes.
pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    if is_image(local) && is_image(remote) {
        return (local == remote).then(|| local.to_vec());
    }
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
        Ordering::Equal => local.max(remote) == local,
        order => order == Ordering::Greater,
    };
    let (winner, loser) = if newer {
        (&left, &right)
    } else {
        (&right, &left)
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
    merged.body = body(
        base.as_ref().map(|b| b.body.as_str()),
        &winner.body,
        &loser.body,
    );
    Some(merged.render().into_bytes())
}

/// The Article of two versions of one Bookmark: if one has none, Drop Article
/// won, unless the Bookmark had none at the last Sync, which makes it Add
/// Article. Two different Articles are the newer side's.
fn body(base: Option<&str>, winner: &str, loser: &str) -> String {
    if winner == loser || loser.is_empty() && base.is_none_or(str::is_empty) {
        winner.to_owned()
    } else if winner.is_empty() && base.is_none_or(str::is_empty) {
        loser.to_owned()
    } else if winner.is_empty() || loser.is_empty() {
        String::new()
    } else {
        winner.to_owned()
    }
}

/// Whether a file of this name is a Bookmark's or an image's: not hidden.
pub fn accepts(file: &str) -> bool {
    !file.starts_with('.') && (file.ends_with(".md") || is_image_file(file))
}

/// Whether `bytes` are a Bookmark or an image.
pub fn is_item(bytes: &[u8]) -> bool {
    is_image(bytes) || parse(bytes).is_some()
}

/// Whether two Bookmarks are one Bookmark: they share an id; two images, if
/// they are the same bytes. None if either isn't a Bookmark or an image, or
/// they differ in kind.
pub fn same_item(a: &[u8], b: &[u8]) -> Option<bool> {
    match (is_image(a), is_image(b)) {
        (true, true) => Some(a == b),
        (false, false) => Some(parse(a)?.scalar("id") == parse(b)?.scalar("id")),
        _ => None,
    }
}

/// What the state files `local` and `remote` merge to; none if either isn't a
/// state.
pub fn merge_state(local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    let (left, right) = (state(local)?, state(remote)?);
    Some(left.merged(&right).encode().into_bytes())
}

/// Whether `bytes` are a state file.
pub fn is_state(bytes: &[u8]) -> bool {
    state(bytes).is_some()
}

fn state(bytes: &[u8]) -> Option<State> {
    State::parse(std::str::from_utf8(bytes).ok()?)
}

/// The Bookmark a file holds: a document with an `id` and an Address.
fn parse(bytes: &[u8]) -> Option<Document> {
    let file = Document::parse(std::str::from_utf8(bytes).ok()?)?;
    (file.scalar("id").is_some() && file.scalar("address").is_some()).then_some(file)
}

fn updated(file: &Document) -> &str {
    file.scalar(UPDATED).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T1: &str = "2024-01-02T00:00:00Z";
    const T2: &str = "2024-01-03T00:00:00Z";

    fn bookmark(title: &str, tags: &[&str], body: &str, updated: Option<&str>) -> Vec<u8> {
        let mut file = Document::default();
        file.set_scalar("id", "aaaaaaaaaaaa");
        file.set_scalar("address", "https://a.example/");
        file.set_scalar("title", title);
        if !tags.is_empty() {
            let tags = tags.iter().map(|t| t.to_string()).collect();
            file.set("tags", pkm_base::front_matter::Value::List(tags));
        }
        if let Some(updated) = updated {
            file.set_scalar(UPDATED, updated);
        }
        file.body = body.into();
        file.render().into_bytes()
    }

    fn merged(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Document {
        let bytes = merge(base, local, remote).unwrap();
        // Whichever Device merges, the result is the same.
        assert_eq!(merge(base, remote, local).unwrap(), bytes);
        Document::parse(std::str::from_utf8(&bytes).unwrap()).unwrap()
    }

    #[test]
    fn a_rename_and_a_tag_change_are_both_kept() {
        let base = bookmark("Old", &["a"], "text\n", None);
        let renamed = bookmark("New", &["a"], "text\n", Some(T1));
        let tagged = bookmark("Old", &["a", "b"], "text\n", Some(T2));
        let file = merged(Some(&base), &renamed, &tagged);
        assert_eq!(file.scalar("title"), Some("New"));
        assert_eq!(file.list("tags").unwrap(), ["a", "b"]);
        assert_eq!(file.scalar(UPDATED), Some(T2));
    }

    #[test]
    fn a_property_changed_on_both_sides_takes_the_newer_side() {
        let base = bookmark("Old", &[], "", None);
        let older = bookmark("One", &[], "", Some(T1));
        let newer = bookmark("Two", &[], "", Some(T2));
        assert_eq!(
            merged(Some(&base), &older, &newer).scalar("title"),
            Some("Two")
        );
        assert_eq!(merged(None, &older, &newer).scalar("title"), Some("Two"));
    }

    #[test]
    fn drop_article_wins_over_a_body_but_add_article_wins_over_none() {
        let with = bookmark("T", &[], "text\n", None);
        let without = bookmark("T", &[], "", Some(T1));
        assert_eq!(merged(Some(&with), &without, &with).body, "");
        assert_eq!(merged(Some(&without), &without, &with).body, "text\n");
        assert_eq!(merged(None, &without, &with).body, "text\n");
    }

    #[test]
    fn images_merge_only_if_the_same_and_other_bytes_are_left_alone() {
        let png = b"\x89PNG\r\n\x1a\n a";
        assert_eq!(merge(None, png, png).unwrap(), png);
        assert_eq!(merge(None, png, b"\x89PNG\r\n\x1a\n b"), None);
        assert_eq!(merge(None, png, &bookmark("T", &[], "", None)), None);
        assert_eq!(merge(None, b"garbage", b"garbage2"), None);
        assert!(is_item(png) && !is_item(b"garbage"));
        assert_eq!(same_item(png, png), Some(true));
        assert_eq!(same_item(&bookmark("T", &[], "", None), png), None);
        assert!(accepts("aaaaaaaaaaaa.md") && accepts("aaaaaaaaaaaa.png"));
        assert!(!accepts(".hidden.md") && !accepts("aaaaaaaaaaaa.json"));
    }

    #[test]
    fn bookmarks_of_different_ids_do_not_merge() {
        let a = bookmark("T", &[], "", None);
        let b = String::from_utf8(a.clone())
            .unwrap()
            .replace("aaaaaaaaaaaa", "bbbbbbbbbbbb");
        assert_eq!(merge(None, &a, b.as_bytes()), None);
        assert_eq!(same_item(&a, b.as_bytes()), Some(false));
    }

    #[test]
    fn states_merge_and_bytes_that_are_no_state_are_left_alone() {
        let a = br#"{"markup":{"highlights":[],"page_comments":[]},"progress":{"last_read":1,"offset":2}}"#;
        let b = br#"{"markup":{"highlights":[],"page_comments":[]},"progress":{"last_read":5,"offset":9}}"#;
        let merged = String::from_utf8(merge_state(a, b).unwrap()).unwrap();
        assert!(merged.contains(r#""offset":9"#), "{merged}");
        assert_eq!(merge_state(a, b"garbage"), None);
        assert!(is_state(a) && !is_state(b"garbage"));
    }
}
