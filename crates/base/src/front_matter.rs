//! Front matter (ADR-0021): the properties at the top of a Markdown file, a
//! YAML subset of scalars (`key: value`) and block lists (`- item` lines)
//! only, so any Obsidian-like tool reads them. Keys the caller doesn't know
//! keep their place and value when the file is written again.
//!
//! ```text
//! ---
//! title: Buy milk
//! tags:
//!   - home
//! ---
//! The body.
//! ```

const FENCE: &str = "---";

/// The value of one property.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Scalar(String),
    List(Vec<String>),
}

/// A Markdown file: its properties in file order, and its body.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Document {
    entries: Vec<(String, Value)>,
    pub body: String,
}

impl Document {
    /// Reads `text`. A file without front matter is all body; a front matter
    /// that is no subset of YAML (nested maps, flow lists, no closing fence)
    /// makes the file unreadable: `None`.
    pub fn parse(text: &str) -> Option<Self> {
        let Some(rest) = text
            .strip_prefix(FENCE)
            .and_then(|r| r.strip_prefix("\r\n").or_else(|| r.strip_prefix('\n')))
        else {
            return Some(Self {
                entries: Vec::new(),
                body: text.to_owned(),
            });
        };
        let mut entries: Vec<(String, Value)> = Vec::new();
        let mut offset = 0;
        for line in rest.split_inclusive('\n') {
            offset += line.len();
            let line = line.trim_end_matches(['\n', '\r']);
            if line == FENCE {
                let body = rest[offset..].to_owned();
                return Some(Self { entries, body });
            }
            if line.trim().is_empty() {
                continue;
            }
            if let Some(item) = line.trim_start().strip_prefix("- ") {
                let Some((_, Value::List(items))) = entries.last_mut() else {
                    return None;
                };
                items.push(unquote(item.trim())?);
            } else {
                let (key, value) = line.split_once(':')?;
                if key.is_empty() || key.starts_with(' ') || key.contains(char::is_whitespace) {
                    return None;
                }
                let value = value.trim();
                let value = if value.is_empty() {
                    // A key with nothing after it opens a list (maybe empty).
                    Value::List(Vec::new())
                } else {
                    Value::Scalar(unquote(value)?)
                };
                entries.push((key.to_owned(), value));
            }
        }
        None
    }

    pub fn scalar(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::Scalar(value) => Some(value),
            Value::List(_) => None,
        }
    }

    pub fn list(&self, key: &str) -> Option<&[String]> {
        match self.get(key)? {
            Value::List(items) => Some(items),
            Value::Scalar(_) => None,
        }
    }

    /// Sets `key`, in place if it exists, else after the others.
    pub fn set(&mut self, key: &str, value: Value) {
        match self.entries.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => *existing = value,
            None => self.entries.push((key.to_owned(), value)),
        }
    }

    pub fn set_scalar(&mut self, key: &str, value: impl Into<String>) {
        self.set(key, Value::Scalar(value.into()));
    }

    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(k, _)| k != key);
    }

    /// The file's text. A document without properties is just its body.
    pub fn render(&self) -> String {
        if self.entries.is_empty() {
            return self.body.clone();
        }
        let mut out = format!("{FENCE}\n");
        for (key, value) in &self.entries {
            match value {
                Value::Scalar(text) => out += &format!("{key}: {}\n", quote(text)),
                Value::List(items) if items.is_empty() => out += &format!("{key}:\n"),
                Value::List(items) => {
                    out += &format!("{key}:\n");
                    for item in items {
                        out += &format!("  - {}\n", quote(item));
                    }
                }
            }
        }
        out += FENCE;
        out += "\n";
        out += &self.body;
        out
    }

    /// The property keys, in file order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(key, _)| key.as_str())
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }
}

/// Whether `text` reads back as itself when written bare.
fn is_plain(text: &str) -> bool {
    let first = text.chars().next();
    !text.is_empty()
        && text == text.trim()
        && !text.contains(['\n', '\r', '\\'])
        && !text.contains(": ")
        && !text.contains(" #")
        && !text.ends_with(':')
        && !first.is_some_and(|c| "\"'-[]{}#&*!|>%@`?,".contains(c))
}

fn quote(text: &str) -> String {
    if is_plain(text) {
        return text.to_owned();
    }
    let mut out = String::from('"');
    for c in text.chars() {
        match c {
            '"' => out += "\\\"",
            '\\' => out += "\\\\",
            '\n' => out += "\\n",
            '\r' => out += "\\r",
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A bare or double-quoted value; `None` for a malformed quoted one.
fn unquote(text: &str) -> Option<String> {
    let Some(inner) = text.strip_prefix('"') else {
        return Some(text.to_owned());
    };
    let inner = inner.strip_suffix('"')?;
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        out.push(match chars.next()? {
            'n' => '\n',
            'r' => '\r',
            c @ ('"' | '\\') => c,
            _ => return None,
        });
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scalars_lists_and_the_body() {
        let doc = Document::parse(
            "---\ntitle: Buy milk\ntags:\n  - home\n  - shop\nempty:\n---\nTwo\nlines\n",
        )
        .unwrap();
        assert_eq!(doc.scalar("title"), Some("Buy milk"));
        assert_eq!(
            doc.list("tags"),
            Some(&["home".to_owned(), "shop".to_owned()][..])
        );
        assert_eq!(doc.list("empty"), Some(&[][..]));
        assert_eq!(doc.body, "Two\nlines\n");
    }

    #[test]
    fn a_file_without_front_matter_is_all_body() {
        let doc = Document::parse("just text\n").unwrap();
        assert_eq!(doc.scalar("title"), None);
        assert_eq!(doc.body, "just text\n");
        assert_eq!(doc.render(), "just text\n");
    }

    #[test]
    fn unreadable_front_matter_is_none() {
        for bad in [
            "---\ntitle: open ended\n",
            "---\n- orphan item\n---\n",
            "---\nkey: \"bad \\q escape\"\n---\n",
            "---\nno colon here\n---\n",
        ] {
            assert_eq!(Document::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn round_trips_awkward_values() {
        let mut doc = Document::default();
        for (i, value) in [
            "",
            " padded ",
            "a: b",
            "# hash",
            "- dash",
            "say \"hi\"",
            "back\\slash",
            "two\nlines",
            "x #y",
            "ends:",
            "ünï",
        ]
        .iter()
        .enumerate()
        {
            doc.set_scalar(&format!("k{i}"), *value);
        }
        doc.set(
            "list",
            Value::List(vec!["a: b".into(), "".into(), "plain".into()]),
        );
        doc.body = "---\nbody with a fence\n".into();
        let back = Document::parse(&doc.render()).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn plain_values_are_written_bare() {
        let mut doc = Document::default();
        doc.set_scalar("title", "Buy milk");
        doc.set("tags", Value::List(vec!["home".into()]));
        assert_eq!(doc.render(), "---\ntitle: Buy milk\ntags:\n  - home\n---\n");
    }

    #[test]
    fn unknown_keys_keep_their_place_and_value_on_rewrite() {
        let text = "---\nid: a\ncustom: kept\nlist:\n  - x\ntitle: old\n---\nbody";
        let mut doc = Document::parse(text).unwrap();
        doc.set_scalar("title", "new");
        doc.set_scalar("added", "last");
        assert_eq!(
            doc.render(),
            "---\nid: a\ncustom: kept\nlist:\n  - x\ntitle: new\nadded: last\n---\nbody"
        );
        doc.remove("custom");
        assert_eq!(doc.scalar("custom"), None);
    }

    #[test]
    fn reads_windows_line_endings() {
        let doc = Document::parse("---\r\ntitle: A\r\n---\r\nbody").unwrap();
        assert_eq!(doc.scalar("title"), Some("A"));
        assert_eq!(doc.body, "body");
    }
}
