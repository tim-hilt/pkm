//! Where a Context's files live: the little a store needs of a file system. The disk
//! is one (`DiskFiles`); memory is the other (`MemoryFiles`), for clients
//! whose storage can't be reached synchronously, like the browser's.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;
use std::fs;
use std::io;
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

/// Files in folders. Folders exist as far as files are in them.
pub trait Files: Send + Sync + Debug {
    /// The file's bytes; `NotFound` if there is no such file.
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>>;

    /// Replaces the file, making its folders. It is never seen half written.
    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;

    /// The file's text; `NotFound` if there is no such file.
    fn read(&self, path: &Path) -> io::Result<String> {
        String::from_utf8(self.read_bytes(path)?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// Replaces the file as `write_bytes` does.
    fn write(&self, path: &Path, text: &str) -> io::Result<()> {
        self.write_bytes(path, text.as_bytes())
    }

    fn exists(&self, path: &Path) -> bool;

    /// Deletes the file; `NotFound` if there is no such file.
    fn remove(&self, path: &Path) -> io::Result<()>;

    /// The files directly in `folder`; none if there is no such folder.
    fn list(&self, folder: &Path) -> io::Result<Vec<Listed>>;

    /// The names of the folders directly in `folder`, empty ones too; none
    /// if there is no such folder.
    fn list_folders(&self, folder: &Path) -> io::Result<Vec<String>>;

    /// Makes the folder and the ones above it; one there already is as wanted.
    fn make_folder(&self, folder: &Path) -> io::Result<()>;

    /// Moves a file or folder with all in it to `to`, whose folder exists.
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;

    /// Deletes the folder with everything in it; one that is gone already is
    /// as wanted.
    fn remove_folder(&self, folder: &Path) -> io::Result<()>;
}

/// The real file system.
#[derive(Debug)]
pub struct DiskFiles;

impl Files for DiskFiles {
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }

    /// Goes through a hidden scratch file, so a crash leaves no torn file and
    /// Sync never sees half of one.
    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let scratch = path.with_file_name(format!(".{name}.tmp"));
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder)?;
        }
        fs::write(&scratch, bytes)
            .and_then(|()| fs::rename(&scratch, path))
            .inspect_err(|_| drop(fs::remove_file(&scratch)))
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn remove(&self, path: &Path) -> io::Result<()> {
        fs::remove_file(path)
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

    fn list_folders(&self, folder: &Path) -> io::Result<Vec<String>> {
        let items = match fs::read_dir(folder) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            items => items?,
        };
        let mut names = Vec::new();
        for item in items {
            let item = item?;
            if item.file_type()?.is_dir() {
                names.extend(item.file_name().to_str().map(str::to_owned));
            }
        }
        Ok(names)
    }

    fn make_folder(&self, folder: &Path) -> io::Result<()> {
        fs::create_dir_all(folder)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }

    fn remove_folder(&self, folder: &Path) -> io::Result<()> {
        match fs::remove_dir_all(folder) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }
}

/// Where `path`, in or at `from`, is after `from` is renamed to `to`.
fn moved_to(path: &Path, from: &Path, to: &Path) -> PathBuf {
    match path.strip_prefix(from) {
        Ok(rest) if !rest.as_os_str().is_empty() => to.join(rest),
        _ => to.to_owned(),
    }
}

#[derive(Debug, Default)]
struct Memory {
    /// Each file's bytes, and when it was last written: a count of writes, so
    /// that it never repeats.
    files: BTreeMap<PathBuf, (Vec<u8>, u128)>,
    writes: u128,
    /// Files written since `take_changes`.
    changed: BTreeSet<PathBuf>,
    /// Folders made without a file in them (yet).
    folders: BTreeSet<PathBuf>,
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
        self.load_bytes(path, text.into_bytes());
    }

    /// `load` for a file that is no text.
    pub fn load_bytes(&self, path: &Path, bytes: Vec<u8>) {
        let mut memory = self.memory();
        memory.writes += 1;
        let written = memory.writes;
        memory.files.insert(path.to_owned(), (bytes, written));
    }

    /// The files written since the last call, each once, sorted.
    pub fn take_changes(&self) -> Vec<PathBuf> {
        std::mem::take(&mut self.memory().changed)
            .into_iter()
            .collect()
    }
}

impl Files for MemoryFiles {
    fn read_bytes(&self, path: &Path) -> io::Result<Vec<u8>> {
        self.memory()
            .files
            .get(path)
            .map(|(bytes, _)| bytes.clone())
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        self.load_bytes(path, bytes.to_owned());
        self.memory().changed.insert(path.to_owned());
        Ok(())
    }

    fn exists(&self, path: &Path) -> bool {
        self.memory().files.contains_key(path)
    }

    /// Counts as a change, even for a file not loaded (an Image the browser
    /// keeps without holding it): the path is named by `take_changes`, and then
    /// reads as missing.
    fn remove(&self, path: &Path) -> io::Result<()> {
        let mut memory = self.memory();
        let held = memory.files.remove(path);
        memory.changed.insert(path.to_owned());
        held.map(drop).ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn list(&self, folder: &Path) -> io::Result<Vec<Listed>> {
        Ok(self
            .memory()
            .files
            .iter()
            .filter(|(path, _)| path.parent() == Some(folder))
            .filter_map(|(path, (bytes, written))| {
                Some(Listed {
                    name: path.file_name()?.to_str()?.to_owned(),
                    stamp: (*written, bytes.len() as u64),
                })
            })
            .collect())
    }

    fn list_folders(&self, folder: &Path) -> io::Result<Vec<String>> {
        let memory = self.memory();
        let holding = memory.files.keys().filter_map(|path| path.parent());
        let made = memory.folders.iter().map(PathBuf::as_path);
        Ok(holding
            .chain(made)
            .filter_map(|dir| dir.strip_prefix(folder).ok()?.components().next())
            .map(|name| name.as_os_str().to_string_lossy().into_owned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect())
    }

    fn make_folder(&self, folder: &Path) -> io::Result<()> {
        self.memory().folders.insert(folder.to_owned());
        Ok(())
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        let mut memory = self.memory();
        let moved: Vec<_> = (memory.files.keys())
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        let folders: Vec<_> = (memory.folders.iter())
            .filter(|path| path.starts_with(from))
            .cloned()
            .collect();
        if moved.is_empty() && folders.is_empty() {
            return Err(io::ErrorKind::NotFound.into());
        }
        for path in moved {
            let held = memory.files.remove(&path);
            let target = moved_to(&path, from, to);
            memory.writes += 1;
            let written = memory.writes;
            if let Some((bytes, _)) = held {
                memory.files.insert(target.clone(), (bytes, written));
            }
            memory.changed.extend([path, target]);
        }
        for path in folders {
            memory.folders.remove(&path);
            memory.folders.insert(moved_to(&path, from, to));
        }
        Ok(())
    }

    fn remove_folder(&self, folder: &Path) -> io::Result<()> {
        let mut memory = self.memory();
        let gone: Vec<_> = (memory.files.keys())
            .filter(|path| path.starts_with(folder))
            .cloned()
            .collect();
        for path in gone {
            memory.files.remove(&path);
            memory.changed.insert(path);
        }
        memory.folders.retain(|path| !path.starts_with(folder));
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_lists_the_files_directly_in_a_folder() {
        let files = MemoryFiles::default();
        files.write(Path::new("Tasks/a.md"), "a").unwrap();
        files.write(Path::new("Tasks/sub/x.md"), "x").unwrap();
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
    fn renaming_a_file_leaves_no_folder_of_its_name() {
        let files = MemoryFiles::default();
        files.write(Path::new("Notes/a.md"), "a").unwrap();
        files.make_folder(Path::new("Notes/Work")).unwrap();
        files
            .rename(Path::new("Notes/a.md"), Path::new("Notes/Work/a.md"))
            .unwrap();
        assert_eq!(files.read(Path::new("Notes/Work/a.md")).unwrap(), "a");
        assert_eq!(files.list_folders(Path::new("Notes")).unwrap(), ["Work"]);
        assert!(
            files
                .list_folders(Path::new("Notes/Work"))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn removing_a_file_not_loaded_is_still_a_change() {
        let files = MemoryFiles::default();
        let path = Path::new("Tasks/a.png");
        assert_eq!(
            files.remove(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        assert_eq!(files.take_changes(), [path]);
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
    fn a_removed_file_is_gone_and_reported_as_a_change() {
        let files = MemoryFiles::default();
        let path = Path::new("Filters/a.md");
        files.load(path, "a".into());
        files.remove(path).unwrap();
        assert!(!files.exists(path));
        assert_eq!(files.take_changes(), [path]);
        assert_eq!(
            files.remove(path).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }

    #[test]
    fn changes_are_reported_once_and_loading_is_no_change() {
        let files = MemoryFiles::default();
        files.load(Path::new("Tasks/old.md"), "old".into());
        files.write(Path::new("Tasks/a.md"), "a").unwrap();
        files.write(Path::new("Tasks/a.md"), "a!").unwrap();
        files.write(Path::new("Tasks/b.md"), "b").unwrap();
        assert_eq!(
            files.take_changes(),
            [Path::new("Tasks/a.md"), Path::new("Tasks/b.md")]
        );
        assert!(files.take_changes().is_empty());
        assert_eq!(files.read(Path::new("Tasks/a.md")).unwrap(), "a!");
        assert_eq!(files.read(Path::new("Tasks/old.md")).unwrap(), "old");
    }

    #[test]
    fn memory_folders_exist_when_made_or_holding_files() {
        let files = MemoryFiles::default();
        files.write(Path::new("N/a/x.md"), "x").unwrap();
        files.write(Path::new("N/y.md"), "y").unwrap();
        files.make_folder(Path::new("N/b/c")).unwrap();
        let root = files.list_folders(Path::new("N")).unwrap();
        assert_eq!(root, ["a", "b"]);
        assert_eq!(files.list_folders(Path::new("N/b")).unwrap(), ["c"]);
        assert!(files.list_folders(Path::new("N/a")).unwrap().is_empty());
    }

    #[test]
    fn memory_renames_and_removes_folders_with_their_files() {
        let files = MemoryFiles::default();
        files.write(Path::new("N/a/x.md"), "x").unwrap();
        files.make_folder(Path::new("N/a/e")).unwrap();
        files.take_changes();
        files.rename(Path::new("N/a"), Path::new("N/b")).unwrap();
        assert_eq!(files.read(Path::new("N/b/x.md")).unwrap(), "x");
        assert_eq!(files.list_folders(Path::new("N/b")).unwrap(), ["e"]);
        assert!(!files.exists(Path::new("N/a/x.md")));
        assert_eq!(
            files.take_changes(),
            [Path::new("N/a/x.md"), Path::new("N/b/x.md")]
        );
        files.remove_folder(Path::new("N/b")).unwrap();
        assert!(files.list_folders(Path::new("N")).unwrap().is_empty());
        files.remove_folder(Path::new("N/b")).unwrap();
    }
}
