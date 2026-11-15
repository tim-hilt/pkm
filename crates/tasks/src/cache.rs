//! What this Device remembers of the Task files (ADR-0021): per file, its
//! modification stamp and what it said, until the stamp changes. Listing then
//! reads only files that are new or changed.
//!
//! A Closed Task is kept as a `Node::closed` stub (how it was Closed, place,
//! Title and Parent), which is all that is written to the cache file between launches. An Open
//! Task is kept in memory only and read again on launch: there are few.

use crate::files::{Files, Stamp};
use crate::store::{Closing, Node};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
struct Entry {
    stamp: Stamp,
    /// `None` for a file that is no Task.
    node: Option<Node>,
}

impl Entry {
    fn is_closed(&self) -> bool {
        self.node.as_ref().is_some_and(|n| n.is_closed())
    }
}

#[derive(Debug)]
pub(crate) struct Cache {
    files: Arc<dyn Files>,
    /// Never in the Synced folder. Without one, nothing outlives the process.
    file: Option<PathBuf>,
    /// Keyed by file name. Empty until the first `nodes`, which restores the
    /// Closed Tasks from `file`.
    entries: HashMap<String, Entry>,
    restored: bool,
}

impl Cache {
    pub(crate) fn new(files: Arc<dyn Files>, file: Option<PathBuf>) -> Self {
        Self {
            files,
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
            self.entries = self
                .file
                .as_deref()
                .map_or_else(HashMap::new, |file| restore(self.files.as_ref(), file));
            self.restored = true;
        }
        let mut previous = std::mem::take(&mut self.entries);
        // Only Closed Tasks are persisted: an Open one changing leaves the file.
        let mut stale = false;
        for listed in self.files.list(folder)? {
            let (name, stamp) = (listed.name, listed.stamp);
            if !name.ends_with(".md") {
                continue;
            }
            let entry = match previous.remove(&name) {
                Some(entry) if entry.stamp == stamp => entry,
                old => {
                    let entry = Entry {
                        stamp,
                        node: read(&folder.join(&name)),
                    };
                    stale |= entry.is_closed() || old.is_some_and(|o| o.is_closed());
                    entry
                }
            };
            self.entries.insert(name, entry);
        }
        if let Some(file) = &self.file
            && (stale || previous.values().any(Entry::is_closed))
        {
            drop(persist(self.files.as_ref(), file, &self.entries)); // only a speed-up
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

/// One Closed Task per line, tab-separated, the Title last:
/// `name mtime size id state parent order title`.
fn persist(files: &dyn Files, file: &Path, entries: &HashMap<String, Entry>) -> io::Result<()> {
    let mut text = String::new();
    for (name, entry) in entries {
        if let Some(node) = entry.node.as_ref().filter(|n| n.is_closed()) {
            let (mtime, size) = entry.stamp;
            let task = &node.task;
            let state = node.closing.map_or("", Closing::state);
            let parent = task.parent.as_deref().unwrap_or("");
            let order = node.order;
            let (id, title) = (&task.id, &task.title);
            text.push_str(&format!(
                "{name}\t{mtime}\t{size}\t{id}\t{state}\t{parent}\t{order}\t{title}\n"
            ));
        }
    }
    files.write(file, &text)
}

/// The Closed Tasks `persist` wrote; lines that don't parse are dropped.
fn restore(files: &dyn Files, file: &Path) -> HashMap<String, Entry> {
    let text = files.read(file).unwrap_or_default();
    text.lines().filter_map(restore_line).collect()
}

fn restore_line(line: &str) -> Option<(String, Entry)> {
    let mut parts = line.splitn(8, '\t');
    let name = parts.next()?;
    let stamp = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    let id = parts.next()?;
    let closing = Closing::parse(parts.next()?)?;
    let parent = Some(parts.next()?).filter(|p| !p.is_empty());
    let order = parts.next()?.parse().ok()?;
    let title = parts.next()?;
    let node = Node::closed(
        id.to_owned(),
        closing,
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
