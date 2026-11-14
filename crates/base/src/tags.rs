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

#[derive(Debug, Clone, PartialEq, Eq)]
struct Meta {
    color: Option<String>,
    /// When the Tag or its color last changed, as a Task's `updated`.
    updated: String,
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
        let value = json::parse(text).unwrap_or(Value::Null);
        let tags = value.get("tags").and_then(Value::as_object);
        let mut found = BTreeMap::new();
        for (name, meta) in tags.into_iter().flat_map(Object::iter) {
            if crate::tag::is_valid(name) {
                let text = |key| meta.get(key).and_then(Value::as_str).map(str::to_owned);
                let (color, updated) = (text("color"), text("updated").unwrap_or_default());
                found.insert(name.to_owned(), Meta { color, updated });
            }
        }
        Self(found)
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
        if self.0.contains_key(tag) {
            return false;
        }
        let meta = Meta {
            color: None,
            updated: updated.to_owned(),
        };
        self.0.insert(tag.to_owned(), meta);
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
    fn a_forgotten_tag_is_gone() {
        let mut tags = tags(&["a", "b"]);
        assert!(tags.forget("a"));
        assert!(!tags.forget("a"));
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
}
