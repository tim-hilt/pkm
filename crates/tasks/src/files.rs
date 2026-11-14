//! Where the Tasks live: the little the store needs of a file system. The disk
//! is one (`DiskFiles`); memory is the other (`MemoryFiles`), for clients
//! whose storage can't be reached synchronously, like the browser's.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};
use std::time::UNIX_EPOCH;

/// Changes whenever a file does: modification time in nanoseconds, and size.
pub type Stamp = (u128, u64);

/// A file in a folder.
pub struct Listed {
    pub name: String,
    pub stamp: Stamp,
}

/// Text files in folders. Folders exist as far as files are in them.
pub trait Files: Send + Sync + Debug {
    /// The file's text; `NotFound` if there is no such file.
    fn read(&self, path: &Path) -> io::Result<String>;

    /// Replaces the file, making its folders. It is never seen half written.
    fn write(&self, path: &Path, text: &str) -> io::Result<()>;

    /// Adds `text` to the end of the file, creating it and its folders.
    fn append(&self, path: &Path, text: &str) -> io::Result<()>;

    fn exists(&self, path: &Path) -> bool;

    /// The files directly in `folder`; none if there is no such folder.
    fn list(&self, folder: &Path) -> io::Result<Vec<Listed>>;
}

/// The real file system.
#[derive(Debug)]
pub struct DiskFiles;

impl Files for DiskFiles {
    fn read(&self, path: &Path) -> io::Result<String> {
        fs::read_to_string(path)
    }

    /// Goes through a hidden scratch file, so a crash leaves no torn file and
    /// Sync never sees half of one.
    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let scratch = path.with_file_name(format!(".{name}.tmp"));
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(&scratch, text)
            .and_then(|()| fs::rename(&scratch, path))
            .inspect_err(|_| drop(fs::remove_file(&scratch)))
    }

    fn append(&self, path: &Path, text: &str) -> io::Result<()> {
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(text.as_bytes())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn list(&self, folder: &Path) -> io::Result<Vec<Listed>> {
        let mut listed = Vec::new();
        let items = match fs::read_dir(folder) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(listed),
            items => items?,
        };
        for item in items {
            let item = item?;
            let Some(name) = item.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let stamp = item.metadata().ok().filter(|m| m.is_file()).and_then(|m| {
                let nanos = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
                Some((nanos.as_nanos(), m.len()))
            });
            if let Some(stamp) = stamp {
                listed.push(Listed { name, stamp });
            }
        }
        Ok(listed)
    }
}

#[derive(Debug, Default)]
struct Memory {
    /// Each file's text, and when it was last written: a count of writes, so
    /// that it never repeats.
    files: BTreeMap<PathBuf, (String, u128)>,
    writes: u128,
    /// Files written since `take_changes`.
    changed: BTreeSet<PathBuf>,
}

/// Files in memory, which remember what changed so that a client can save it
/// elsewhere, and can be filled from there (`load`).
#[derive(Debug, Default)]
pub struct MemoryFiles(Mutex<Memory>);

impl MemoryFiles {
    fn memory(&self) -> std::sync::MutexGuard<'_, Memory> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Puts a file there as it was saved before; not counted as a change.
    pub fn load(&self, path: &Path, text: String) {
        let mut memory = self.memory();
        memory.writes += 1;
        let written = memory.writes;
        memory.files.insert(path.to_owned(), (text, written));
    }

    /// The files written since the last call, each once, sorted.
    pub fn take_changes(&self) -> Vec<PathBuf> {
        std::mem::take(&mut self.memory().changed)
            .into_iter()
            .collect()
    }
}

impl Files for MemoryFiles {
    fn read(&self, path: &Path) -> io::Result<String> {
        self.memory()
            .files
            .get(path)
            .map(|(text, _)| text.clone())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        self.load(path, text.to_owned());
        self.memory().changed.insert(path.to_owned());
        Ok(())
    }

    fn append(&self, path: &Path, text: &str) -> io::Result<()> {
        let mut whole = self.read(path).unwrap_or_default();
        whole.push_str(text);
        self.write(path, &whole)
    }

    fn exists(&self, path: &Path) -> bool {
        self.memory().files.contains_key(path)
    }

    fn list(&self, folder: &Path) -> io::Result<Vec<Listed>> {
        Ok(self
            .memory()
            .files
            .iter()
            .filter(|(path, _)| path.parent() == Some(folder))
            .filter_map(|(path, (text, written))| {
                Some(Listed {
                    name: path.file_name()?.to_str()?.to_owned(),
                    stamp: (*written, text.len() as u64),
                })
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_lists_the_files_directly_in_a_folder() {
        let files = MemoryFiles::default();
        files.write(Path::new("Tasks/a.md"), "a").unwrap();
        files.write(Path::new("Tasks/.history/x.log"), "x").unwrap();
        let names: Vec<_> = files
            .list(Path::new("Tasks"))
            .unwrap()
            .into_iter()
            .map(|f| f.name)
            .collect();
        assert_eq!(names, ["a.md"]);
        assert!(files.list(Path::new("Elsewhere")).unwrap().is_empty());
    }

    #[test]
    fn a_rewritten_file_gets_a_new_stamp() {
        let files = MemoryFiles::default();
        let path = Path::new("Tasks/a.md");
        files.write(path, "a").unwrap();
        let before = files.list(Path::new("Tasks")).unwrap()[0].stamp;
        files.write(path, "b").unwrap();
        assert_ne!(files.list(Path::new("Tasks")).unwrap()[0].stamp, before);
    }

    #[test]
    fn changes_are_reported_once_and_loading_is_no_change() {
        let files = MemoryFiles::default();
        files.load(Path::new("Tasks/old.md"), "old".into());
        files.write(Path::new("Tasks/a.md"), "a").unwrap();
        files.append(Path::new("Tasks/a.md"), "!").unwrap();
        files.write(Path::new("Tasks/b.md"), "b").unwrap();
        assert_eq!(
            files.take_changes(),
            [Path::new("Tasks/a.md"), Path::new("Tasks/b.md")]
        );
        assert!(files.take_changes().is_empty());
        assert_eq!(files.read(Path::new("Tasks/a.md")).unwrap(), "a!");
        assert_eq!(files.read(Path::new("Tasks/old.md")).unwrap(), "old");
    }
}
