//! The Tags file on disk (ADR-0023), shared by every Context that puts Tags
//! on Items: it reads, changes and writes `Tags/tags.json` beside the
//! Context's folder.

use crate::clock::{iso8601, now_ms};
use crate::files::Files;
use crate::tags::Tags;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug)]
pub struct TagStore {
    files: Arc<dyn Files>,
    path: PathBuf,
    /// Held while the file is read and written back.
    lock: Mutex<()>,
}

impl TagStore {
    /// The Tags beside the Context folder `root`, on `files`.
    pub fn beside(files: Arc<dyn Files>, root: &Path) -> Self {
        Self {
            files,
            path: root.parent().unwrap_or(Path::new("")).join(Tags::FILE),
            lock: Mutex::new(()),
        }
    }

    /// The Tags; none if there is no file or it isn't one.
    pub fn read(&self) -> Tags {
        Tags::parse(&self.files.read(&self.path).unwrap_or_default())
    }

    /// Adds the valid `found` Tags, leaving known ones as they are; reads
    /// nothing if none are found, writes nothing if all are known.
    pub fn remember(&self, found: impl IntoIterator<Item = String>) -> io::Result<()> {
        let mut found = found.into_iter().peekable();
        if found.peek().is_none() {
            return Ok(());
        }
        let updated = iso8601(now_ms());
        self.change(|tags| {
            let mut changed = false;
            for tag in found {
                changed |= tags.add(&tag, &updated);
            }
            Ok(changed)
        })
    }

    /// Raises the Relevance of the valid `tags`, just put on an Item, adding
    /// the unknown ones; reads nothing if there are none.
    pub fn put_on(&self, tags: impl IntoIterator<Item = String>) -> io::Result<()> {
        let mut tags = tags.into_iter().peekable();
        if tags.peek().is_none() {
            return Ok(());
        }
        let updated = iso8601(now_ms());
        self.change(|known| {
            for tag in tags {
                known.raise(&tag, &updated);
            }
            Ok(true)
        })
    }

    /// Reads the Tags, lets `change` change them and writes them back if it
    /// says it did, all under one lock.
    pub fn change(&self, change: impl FnOnce(&mut Tags) -> io::Result<bool>) -> io::Result<()> {
        let _held = self.lock.lock().unwrap_or_else(PoisonError::into_inner);
        let mut tags = self.read();
        if change(&mut tags)? {
            self.files.write(&self.path, &tags.render())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::MemoryFiles;

    fn store() -> TagStore {
        TagStore::beside(Arc::new(MemoryFiles::default()), Path::new("/data/Tasks"))
    }

    #[test]
    fn remembered_tags_are_known_beside_the_context_folder() {
        let store = store();
        store.remember(["b".to_owned(), "a".to_owned()]).unwrap();
        assert_eq!(store.read().names().collect::<Vec<_>>(), ["a", "b"]);
        assert!(store.files.exists(Path::new("/data/Tags/tags.json")));
    }

    #[test]
    fn tags_put_on_an_item_are_known_and_raised() {
        let store = store();
        store.remember(["a".to_owned()]).unwrap();
        store.put_on(["a".to_owned(), "b".to_owned()]).unwrap();
        store.put_on(["a".to_owned()]).unwrap();
        let tags = store.read();
        assert_eq!((tags.relevance("a"), tags.relevance("b")), (2, 1));
    }

    #[test]
    fn remembering_nothing_new_writes_nothing() {
        let store = store();
        store.remember(Vec::new()).unwrap();
        store.put_on(Vec::new()).unwrap();
        assert!(!store.files.exists(&store.path));
    }
}
