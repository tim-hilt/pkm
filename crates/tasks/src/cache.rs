//! What this Device remembers of the Task files (ADR-0021): per file, its
//! modification stamp and what it said, until the stamp changes. Listing then
//! reads only files that are new or changed.
//!
//! A Closed Task is kept as a `Node::closed` stub (place, Title and Parent),
//! which is all that is written to the cache file between launches. An Open
//! Task is kept in memory only and read again on launch: there are few.

use crate::store::Node;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

/// Changes whenever a file does: modification time in nanoseconds, and size.
type Stamp = (u128, u64);

#[derive(Debug)]
struct Entry {
    stamp: Stamp,
    /// `None` for a file that is no Task.
    node: Option<Node>,
}

impl Entry {
    fn is_closed(&self) -> bool {
        self.node.as_ref().is_some_and(|n| n.task.closed)
    }
}

#[derive(Debug)]
pub(crate) struct Cache {
    /// Never in the Synced folder.
    file: PathBuf,
    /// Keyed by file name. Empty until the first `nodes`, which restores the
    /// Closed Tasks from `file`.
    entries: HashMap<String, Entry>,
    restored: bool,
}

impl Cache {
    pub(crate) fn new(file: PathBuf) -> Self {
        Self {
            file,
            entries: HashMap::new(),
            restored: false,
        }
    }

    /// Every Task in `folder`, by Item ID. `read` parses one file; it is called
    /// only for files whose stamp differs from the one remembered. A file that
    /// disappeared is forgotten.
    pub(crate) fn nodes(
        &mut self,
        folder: &Path,
        read: impl Fn(&Path) -> Option<Node>,
    ) -> io::Result<HashMap<String, Node>> {
        if !self.restored {
            self.entries = restore(&self.file);
            self.restored = true;
        }
        let mut previous = std::mem::take(&mut self.entries);
        // Only Closed Tasks are persisted: an Open one changing leaves the file.
        let mut stale = false;
        for item in fs::read_dir(folder)? {
            let path = item?.path();
            let (Some(name), true) = (
                path.file_name().and_then(|n| n.to_str()),
                path.extension().is_some_and(|e| e == "md"),
            ) else {
                continue;
            };
            let Some(stamp) = stamp_of(&path) else {
                continue;
            };
            let entry = match previous.remove(name) {
                Some(entry) if entry.stamp == stamp => entry,
                old => {
                    let entry = Entry {
                        stamp,
                        node: read(&path),
                    };
                    stale |= entry.is_closed() || old.is_some_and(|o| o.is_closed());
                    entry
                }
            };
            self.entries.insert(name.to_owned(), entry);
        }
        if stale || previous.values().any(Entry::is_closed) {
            drop(persist(&self.file, &self.entries)); // only a speed-up
        }
        Ok(self
            .entries
            .values()
            .filter_map(|e| e.node.clone())
            .map(|n| (n.task.id.clone(), n))
            .collect())
    }

    /// Reads the file called `name` again next time: its owner just wrote it.
    pub(crate) fn forget(&mut self, name: &str) {
        self.entries.remove(name);
    }
}

fn stamp_of(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    let nanos = meta.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    Some((nanos.as_nanos(), meta.len()))
}

/// One Closed Task per line, tab-separated, the Title last:
/// `name mtime size id parent order title`.
fn persist(file: &Path, entries: &HashMap<String, Entry>) -> io::Result<()> {
    let mut text = String::new();
    for (name, entry) in entries {
        if let Some(node) = entry.node.as_ref().filter(|n| n.task.closed) {
            let (mtime, size) = entry.stamp;
            let task = &node.task;
            let parent = task.parent.as_deref().unwrap_or("");
            let order = node.order;
            let (id, title) = (&task.id, &task.title);
            text.push_str(&format!(
                "{name}\t{mtime}\t{size}\t{id}\t{parent}\t{order}\t{title}\n"
            ));
        }
    }
    let scratch = file.with_extension("tmp");
    fs::write(&scratch, text).and_then(|()| fs::rename(&scratch, file))
}

/// The Closed Tasks `persist` wrote; lines that don't parse are dropped.
fn restore(file: &Path) -> HashMap<String, Entry> {
    let text = fs::read_to_string(file).unwrap_or_default();
    text.lines().filter_map(restore_line).collect()
}

fn restore_line(line: &str) -> Option<(String, Entry)> {
    let mut parts = line.splitn(7, '\t');
    let name = parts.next()?;
    let stamp = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let id = parts.next()?;
    let parent = Some(parts.next()?).filter(|p| !p.is_empty());
    let order = parts.next()?.parse().ok()?;
    let title = parts.next()?;
    let node = Node::closed(
        id.to_owned(),
        title.to_owned(),
        parent.map(str::to_owned),
        order,
    );
    Some((
        name.to_owned(),
        Entry {
            stamp,
            node: Some(node),
        },
    ))
}
