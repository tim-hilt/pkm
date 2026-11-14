//! The Tags that exist (ADR-0023): `Tags/tags.json`, beside the Context
//! folders. A Tag lives from when it is first put on an Item until it is
//! Deleted, whether or not an Item carries it. Knows nothing about Items or
//! files: callers read and write the text.

use crate::json::{self, Object, Value};
use std::collections::BTreeMap;

/// A Tag and its Tag Color.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// `#rrggbb` (sRGB); `None` is Default.
    pub color: Option<String>,
}

/// Ordered by `updated` first, so that `max` picks the newer; a tie falls to
/// the color, whichever side is local.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Meta {
    /// When the Tag or its color last changed, as a Task's `updated`.
    updated: String,
    color: Option<String>,
}

/// Every known Tag, by name.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tags(BTreeMap<String, Meta>);

impl Tags {
    /// Where the file is, from the folder that holds the Context folders.
    pub const FILE: &'static str = "Tags/tags.json";

    /// The Tags in `text`; none if it isn't the Tags file. Invalid Tag names
    /// are left out.
    pub fn parse(text: &str) -> Self {
        Self::try_parse(text).unwrap_or_default()
    }

    /// Like `parse`, but none if `text` isn't the Tags file.
    fn try_parse(text: &str) -> Option<Self> {
        let value = json::parse(text).ok()?;
        let tags = value.get("tags")?.as_object()?;
        let mut found = BTreeMap::new();
        for (name, meta) in tags.iter() {
            if crate::tag::is_valid(name) {
                let text = |key| meta.get(key).and_then(Value::as_str).map(str::to_owned);
                let (color, updated) = (text("color"), text("updated").unwrap_or_default());
                found.insert(name.to_owned(), Meta { updated, color });
            }
        }
        Some(Self(found))
    }

    /// What the Tags files `local` and `remote` merge to, given the file at the
    /// last Sync (none if there was none). Per Tag: one gone from a side that
    /// had it at the last Sync is gone, even if its Tag Color changed on the
    /// other; otherwise a Tag on either side is kept, and the newer `updated`
    /// decides its Tag Color. None if either isn't a Tags file.
    pub fn merge(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        let (local, remote) = (Self::of_bytes(local)?, Self::of_bytes(remote)?);
        // An unreadable base is as good as none.
        let base = base.and_then(Self::of_bytes);
        let deleted = |name: &String, side: &Self| {
            base.as_ref()
                .is_some_and(|b| b.0.contains_key(name) && !side.0.contains_key(name))
        };
        let mut merged = BTreeMap::new();
        for (name, meta) in local.0.iter().chain(&remote.0) {
            if deleted(name, &local) || deleted(name, &remote) {
                continue;
            }
            merged
                .entry(name.clone())
                .and_modify(|kept: &mut Meta| *kept = meta.max(kept).clone())
                .or_insert_with(|| meta.clone());
        }
        Some(Self(merged).render().into_bytes())
    }

    /// Whether `bytes` are a Tags file.
    pub fn is_file(bytes: &[u8]) -> bool {
        Self::of_bytes(bytes).is_some()
    }

    fn of_bytes(bytes: &[u8]) -> Option<Self> {
        Self::try_parse(std::str::from_utf8(bytes).ok()?)
    }

    /// The file's text, with the Tags sorted.
    pub fn render(&self) -> String {
        let mut tags = Object::new();
        for (name, meta) in &self.0 {
            let mut entry = Object::new();
            if let Some(color) = &meta.color {
                entry.insert("color".into(), Value::String(color.clone()));
            }
            entry.insert("updated".into(), Value::String(meta.updated.clone()));
            tags.insert(name.clone(), Value::Object(entry));
        }
        let mut root = Object::new();
        root.insert("tags".into(), Value::Object(tags));
        Value::Object(root).to_json_pretty() + "\n"
    }

    /// Every Tag, alphabetically.
    pub fn list(&self) -> Vec<Entry> {
        self.0
            .iter()
            .map(|(name, meta)| Entry {
                name: name.clone(),
                color: meta.color.clone(),
            })
            .collect()
    }

    /// Whether `tag` is known.
    pub fn contains(&self, tag: &str) -> bool {
        self.0.contains_key(tag)
    }

    /// The Tag Color of `tag` (`#rrggbb`); `None` for Default or an unknown Tag.
    pub fn color(&self, tag: &str) -> Option<&str> {
        self.0.get(tag)?.color.as_deref()
    }

    /// The names of the Tags starting with `prefix`, alphabetically.
    pub fn with_prefix(&self, prefix: &str) -> Vec<String> {
        self.0
            .range(prefix.to_owned()..)
            .map(|(name, _)| name)
            .take_while(|name| name.starts_with(prefix))
            .cloned()
            .collect()
    }

    /// Adds `tag` (valid, as `updated`) in Default; a known Tag stays as it
    /// is. Whether anything changed.
    pub fn add(&mut self, tag: &str, updated: &str) -> bool {
        if self.contains(tag) {
            return false;
        }
        let meta = Meta {
            updated: updated.to_owned(),
            color: None,
        };
        self.0.insert(tag.to_owned(), meta);
        true
    }

    /// Gives the known `tag` the Tag Color `color` (`#rrggbb`, kept lowercase),
    /// or Default for `None`, and bumps its `updated`. False, changing
    /// nothing, for an unknown Tag or a malformed color.
    pub fn set_color(&mut self, tag: &str, color: Option<&str>, updated: &str) -> bool {
        let is_hex = |c: &str| {
            c.len() == 7 && c.starts_with('#') && c[1..].bytes().all(|b| b.is_ascii_hexdigit())
        };
        if color.is_some_and(|c| !is_hex(c)) {
            return false;
        }
        let Some(meta) = self.0.get_mut(tag) else {
            return false;
        };
        meta.color = color.map(str::to_ascii_lowercase);
        meta.updated = updated.to_owned();
        true
    }

    /// Forgets `tag`, with its Tag Color. Whether it was known.
    pub fn forget(&mut self, tag: &str) -> bool {
        self.0.remove(tag).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: &str = "2026-01-01T00:00:00Z";

    fn tags(names: &[&str]) -> Tags {
        let mut tags = Tags::default();
        for name in names {
            tags.add(name, T);
        }
        tags
    }

    fn names(tags: &Tags) -> Vec<String> {
        tags.list().into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn tags_are_listed_alphabetically() {
        assert_eq!(
            names(&tags(&["work", "home", "a/b"])),
            ["a/b", "home", "work"]
        );
    }

    #[test]
    fn tags_are_found_by_prefix() {
        let tags = tags(&["work/b", "home", "work/a", "w", "x"]);
        assert_eq!(tags.with_prefix("w"), ["w", "work/a", "work/b"]);
        assert_eq!(tags.with_prefix("").len(), 5);
        assert!(tags.with_prefix("y").is_empty());
    }

    #[test]
    fn adding_a_known_tag_leaves_it_as_it_is() {
        let mut tags = Tags::parse(
            r##"{"tags": {"work": {"color": "#d0021b", "updated": "2020-01-01T00:00:00Z"}}}"##,
        );
        assert!(!tags.add("work", T));
        assert!(tags.add("home", T));
        assert_eq!(tags.list()[1].color.as_deref(), Some("#d0021b"));
        assert!(tags.render().contains("2020-01-01T00:00:00Z"));
    }

    #[test]
    fn a_tag_color_is_set_and_reset_bumping_updated() {
        let mut tags = tags(&["work"]);
        assert!(tags.set_color("work", Some("#D0021B"), "2027-02-02T00:00:00Z"));
        assert_eq!(tags.color("work"), Some("#d0021b"));
        assert_eq!(tags.color("unknown"), None);
        assert!(tags.render().contains("2027-02-02T00:00:00Z"));
        assert!(tags.set_color("work", None, "2027-03-03T00:00:00Z"));
        assert_eq!(tags.list()[0].color, None);
        assert!(tags.render().contains("2027-03-03T00:00:00Z"));
        assert!(!tags.render().contains("color"));
    }

    #[test]
    fn a_malformed_color_or_unknown_tag_changes_nothing() {
        let mut tags = tags(&["work"]);
        let before = tags.clone();
        for color in ["red", "#fff", "d0021b", "#d0021g", "#d0021b0"] {
            assert!(!tags.set_color("work", Some(color), "2027-02-02T00:00:00Z"));
        }
        assert!(!tags.set_color("nope", Some("#d0021b"), "2027-02-02T00:00:00Z"));
        assert_eq!(tags, before);
    }

    #[test]
    fn a_forgotten_tag_is_gone() {
        let mut tags = tags(&["a", "b"]);
        assert!(tags.contains("a"));
        assert!(tags.forget("a"));
        assert!(!tags.forget("a"));
        assert!(!tags.contains("a"));
        assert_eq!(names(&tags), ["b"]);
    }

    #[test]
    fn the_file_has_the_documented_shape_and_round_trips() {
        let text = r##"{
  "tags": {
    "home": {
      "updated": "2026-01-01T00:00:00Z"
    },
    "work": {
      "color": "#d0021b",
      "updated": "2026-01-02T00:00:00Z"
    }
  }
}
"##;
        let tags = Tags::parse(text);
        assert_eq!(
            tags.list(),
            [
                Entry {
                    name: "home".into(),
                    color: None
                },
                Entry {
                    name: "work".into(),
                    color: Some("#d0021b".into())
                },
            ]
        );
        assert_eq!(tags.render(), text);
    }

    #[test]
    fn text_that_is_no_tags_file_holds_no_tags() {
        for text in ["", "nope", "[]", r#"{"tags": []}"#] {
            assert!(Tags::parse(text).list().is_empty(), "{text:?}");
        }
        let tags = Tags::parse(r#"{"tags": {"Bad Tag": {}, "ok": {}}}"#);
        assert_eq!(names(&tags), ["ok"]);
    }

    const NEWER: &str = "2027-01-01T00:00:00Z";
    const NEWEST: &str = "2028-01-01T00:00:00Z";

    /// A Tags file of `(name, color, updated)`.
    fn file(entries: &[(&str, Option<&str>, &str)]) -> Vec<u8> {
        let mut tags = Tags::default();
        for (name, color, updated) in entries {
            tags.add(name, updated);
            tags.set_color(name, *color, updated);
        }
        tags.render().into_bytes()
    }

    /// The merge, the same from both sides.
    fn merged(base: Option<&[u8]>, a: &[u8], b: &[u8]) -> Tags {
        let ab = Tags::merge(base, a, b).unwrap();
        assert_eq!(ab, Tags::merge(base, b, a).unwrap(), "symmetric");
        Tags::parse(std::str::from_utf8(&ab).unwrap())
    }

    fn colors(tags: &Tags) -> Vec<(String, Option<String>)> {
        tags.list().into_iter().map(|e| (e.name, e.color)).collect()
    }

    #[test]
    fn a_tag_added_on_one_side_is_kept() {
        let base = file(&[("a", None, T)]);
        let local = file(&[("a", None, T), ("b", Some("#d0021b"), NEWER)]);
        let tags = merged(Some(&base), &local, &base);
        assert_eq!(
            colors(&tags),
            [("a".into(), None), ("b".into(), Some("#d0021b".into()))]
        );
    }

    #[test]
    fn a_tag_deleted_on_one_side_is_gone_even_if_its_color_changed_on_the_other() {
        let base = file(&[("a", None, T), ("b", None, T)]);
        let deleted = file(&[("b", None, T)]);
        let recolored = file(&[("a", Some("#d0021b"), NEWER), ("b", None, T)]);
        assert_eq!(names(&merged(Some(&base), &deleted, &recolored)), ["b"]);
    }

    #[test]
    fn a_color_changed_on_both_sides_takes_the_newer() {
        let base = file(&[("a", None, T)]);
        let one = file(&[("a", Some("#111111"), NEWER)]);
        let two = file(&[("a", Some("#222222"), NEWEST)]);
        let tags = merged(Some(&base), &one, &two);
        assert_eq!(colors(&tags), [("a".into(), Some("#222222".into()))]);
    }

    #[test]
    fn without_a_base_both_sides_tags_are_kept_and_the_newer_color_wins() {
        let one = file(&[("a", Some("#111111"), NEWER), ("b", None, T)]);
        let two = file(&[("a", Some("#222222"), NEWEST), ("c", None, T)]);
        let tags = merged(None, &one, &two);
        assert_eq!(
            colors(&tags),
            [
                ("a".into(), Some("#222222".into())),
                ("b".into(), None),
                ("c".into(), None)
            ]
        );
    }

    #[test]
    fn equal_updated_settles_alike_on_both_sides() {
        let one = file(&[("a", Some("#111111"), NEWER)]);
        let two = file(&[("a", Some("#222222"), NEWER)]);
        merged(None, &one, &two); // `merged` checks the symmetry
    }

    #[test]
    fn only_a_tags_file_merges() {
        let good = file(&[("a", None, T)]);
        assert!(Tags::is_file(&good));
        for bad in [&b"nope"[..], b"[]", br#"{"tags": []}"#, b"\xff"] {
            assert!(!Tags::is_file(bad));
            assert_eq!(Tags::merge(None, &good, bad), None);
            assert_eq!(Tags::merge(None, bad, &good), None);
        }
    }

    #[test]
    fn an_unreadable_base_is_as_good_as_none() {
        let one = file(&[("a", None, T)]);
        let two = file(&[("b", None, T)]);
        assert_eq!(names(&merged(Some(b"junk"), &one, &two)), ["a", "b"]);
    }
}
