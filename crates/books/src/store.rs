//! The Books folder on disk: where everything lives, and how Files and state
//! are matched.
//!
//! ```text
//! <root>/<file name>            a Book's File, as the user knows it
//! <root>/.state/<id>.json       its state (`state`)
//! <root>/.state/preferences.json
//! <root>/.details/<id>          its details, this Device only (ADR-0003)
//! <root>/.staging/              Imports and writes in flight
//! ```
//!
//! The root shows nothing but Books. Hidden entries are the app's.

use crate::details::format_of;
use crate::files::{ignore_not_found, millis, write_atomically};
use crate::state::{BookState, new_id};
use pkm_base::sha256::Sha256;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

const STAGING: &str = ".staging";
const STATE: &str = ".state";
const DETAILS: &str = ".details";
const PREFERENCES: &str = "preferences.json";

/// The Books folder: its layout, and which state belongs to which File.
#[derive(Debug)]
pub(crate) struct Folder {
    root: PathBuf,
}

impl Folder {
    /// Opens the folder at `root`, creating it if needed and discarding what
    /// Imports and writes that never finished left in staging.
    pub(crate) fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let folder = Self { root: root.into() };
        ignore_not_found(fs::remove_dir_all(folder.staging()))?;
        for dir in [folder.staging(), folder.state_dir(), folder.details_dir()] {
            fs::create_dir_all(dir)?;
        }
        Ok(folder)
    }

    pub(crate) fn staging(&self) -> PathBuf {
        self.root.join(STAGING)
    }

    /// Where the File called `name` is, or would be.
    pub(crate) fn file(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    pub(crate) fn preferences(&self) -> PathBuf {
        self.state_dir().join(PREFERENCES)
    }

    /// Where the details of Book `id` are, or would be.
    pub(crate) fn details(&self, id: &str) -> PathBuf {
        self.details_dir().join(id)
    }

    fn state_dir(&self) -> PathBuf {
        self.root.join(STATE)
    }

    fn details_dir(&self) -> PathBuf {
        self.root.join(DETAILS)
    }

    pub(crate) fn state_file(&self, id: &str) -> PathBuf {
        self.state_dir().join(format!("{id}.json"))
    }

    /// Stores `state`, replacing the one of its Book.
    pub(crate) fn write_state(&self, state: &BookState) -> io::Result<()> {
        write_atomically(
            &self.staging(),
            &self.state_file(&state.id),
            &state.encode(),
        )
    }

    /// The name to store a File that is called `wanted` under: `wanted` itself,
    /// or with " 2", then " 3"... before the extension if that is taken. Never
    /// hidden.
    pub(crate) fn free_name(&self, wanted: &str) -> String {
        let wanted = wanted.trim_start_matches('.');
        let wanted = if wanted.is_empty() { "Book" } else { wanted };
        let (stem, extension) = match wanted.rfind('.') {
            Some(dot) if dot > 0 => wanted.split_at(dot),
            _ => (wanted, ""),
        };
        let mut name = wanted.to_owned();
        let mut number = 2;
        while self.file(&name).symlink_metadata().is_ok() {
            name = format!("{stem} {number}{extension}");
            number += 1;
        }
        name
    }

    /// Deletes Book `state`: its File, then its state, so a crash in between
    /// leaves a state without a File, which is a deleted Book, not a File that
    /// would be a new one.
    pub(crate) fn remove(&self, state: &BookState) -> io::Result<()> {
        ignore_not_found(fs::remove_file(self.file(&state.file_name)))?;
        self.remove_state(&state.id)
    }

    /// Deletes what is kept of Book `id` beside its File.
    fn remove_state(&self, id: &str) -> io::Result<()> {
        ignore_not_found(fs::remove_file(self.state_file(id)))?;
        ignore_not_found(fs::remove_file(self.details(id)))
    }

    /// The Books in the folder, each with its state, matched like this:
    ///
    /// 1. States of the same File (same SHA-256), made by Imports on two
    ///    Devices before they synced, become the one of the lowest Item ID.
    ///    The others' states go, and their Files too, unless they are the
    ///    only copy.
    /// 2. A File belongs to the state naming it.
    /// 3. Else, to the state whose File is gone and that has the File's
    ///    SHA-256: it was renamed outside the app, and the state follows.
    /// 4. Else it is a new Book with a new Item ID, imported when the File
    ///    was created. A File edited and renamed at once is one, too.
    ///
    /// What is left, a state without a File, is a Book that was deleted: its
    /// state goes. Anything that is neither a PDF nor an EPUB is no Book.
    /// Changes to state are written before this returns.
    pub(crate) fn scan(&self) -> io::Result<Vec<BookState>> {
        let mut files = self.files()?;
        let states = self.merge_duplicates(self.read_states()?, &mut files)?;
        let (mut books, mut lost): (Vec<_>, Vec<_>) = states
            .into_iter()
            .partition(|state| files.remove(&state.file_name).is_some());
        for (name, created) in files {
            let path = self.file(&name);
            if !matches!(format_of(&path), Ok(Some(_))) {
                continue;
            }
            // A File that can't be read now is looked at again by the next scan.
            let Ok(sha256) = hash_file(&path) else {
                continue;
            };
            let state = match lost.iter().position(|state| state.sha256 == sha256) {
                Some(i) => BookState {
                    file_name: name,
                    ..lost.swap_remove(i)
                },
                None => BookState {
                    id: new_id(),
                    file_name: name,
                    sha256,
                    imported: created,
                    markup: Default::default(),
                    progress: None,
                },
            };
            self.write_state(&state)?;
            books.push(state);
        }
        for state in lost {
            self.remove_state(&state.id)?;
        }
        Ok(books)
    }

    /// The names of the Files in the folder, with when they were created.
    fn files(&self) -> io::Result<BTreeMap<String, u64>> {
        let mut files = BTreeMap::new();
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !entry.file_type()?.is_file() {
                continue;
            }
            let meta = entry.metadata()?;
            let created = meta.created().or_else(|_| meta.modified())?;
            files.insert(name, millis(created));
        }
        Ok(files)
    }

    /// The stored states, unreadable ones deleted.
    fn read_states(&self) -> io::Result<Vec<BookState>> {
        let mut states = Vec::new();
        for entry in fs::read_dir(self.state_dir())? {
            let path = entry?.path();
            let is_state = path.extension().is_some_and(|e| e == "json")
                && path.file_name().is_some_and(|n| n != PREFERENCES);
            if !is_state {
                continue;
            }
            match fs::read(&path).map(|bytes| BookState::decode(&bytes)) {
                Ok(Some(state)) => states.push(state),
                Ok(None) => ignore_not_found(fs::remove_file(&path))?,
                Err(_) => {}
            }
        }
        Ok(states)
    }

    /// `states` with those of the same File merged into the lowest Item ID,
    /// see `scan`. Removes from `files` what it deletes.
    fn merge_duplicates(
        &self,
        mut states: Vec<BookState>,
        files: &mut BTreeMap<String, u64>,
    ) -> io::Result<Vec<BookState>> {
        states.sort_by(|a, b| a.id.cmp(&b.id));
        let mut kept: Vec<BookState> = Vec::new();
        let mut by_hash: HashMap<String, usize> = HashMap::new();
        for state in states {
            let Some(&i) = by_hash.get(&state.sha256) else {
                by_hash.insert(state.sha256.clone(), kept.len());
                kept.push(state);
                continue;
            };
            let winner = &mut kept[i];
            let name = state.file_name.clone();
            if name != winner.file_name && files.contains_key(&name) {
                if !files.contains_key(&winner.file_name) {
                    winner.file_name = name;
                } else if hash_file(&self.file(&name))? == winner.sha256 {
                    fs::remove_file(self.file(&name))?;
                    files.remove(&name);
                }
            }
            let id = state.id.clone();
            winner.absorb(state);
            self.write_state(winner)?;
            self.remove_state(&id)?;
        }
        Ok(kept)
    }
}

/// The SHA-256 of the file at `path`, lowercase hex.
fn hash_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; 256 * 1024];
    loop {
        match file.read(&mut buffer) {
            Ok(0) => return Ok(hasher.finalize_hex()),
            Ok(n) => hasher.update(&buffer[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::unique_name;
    use crate::markup::Markup;

    /// A fresh Books folder under the system temp dir, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            Self(std::env::temp_dir().join(format!("pkm-books-store-{}", unique_name())))
        }

        fn folder(&self) -> Folder {
            Folder::open(&self.0).unwrap()
        }

        fn put(&self, name: &str, bytes: &[u8]) {
            fs::write(self.0.join(name), bytes).unwrap();
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    const A: &[u8] = b"%PDF-a";
    const B: &[u8] = b"%PDF-b";

    fn sha(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        hasher.finalize_hex()
    }

    fn state(id: &str, name: &str, bytes: &[u8]) -> BookState {
        BookState {
            id: id.into(),
            file_name: name.into(),
            sha256: sha(bytes),
            imported: 5,
            markup: Markup::default(),
            progress: None,
        }
    }

    fn names(books: &[BookState]) -> Vec<(&str, &str)> {
        let mut names: Vec<_> = books
            .iter()
            .map(|b| (b.id.as_str(), b.file_name.as_str()))
            .collect();
        names.sort();
        names
    }

    fn visible(scratch: &Scratch) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(&scratch.0)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn a_file_belongs_to_the_state_that_names_it_whatever_its_content() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        folder
            .write_state(&state("aaaaaaaaaaaa", "a.pdf", A))
            .unwrap();
        // Edited outside the app: same name, other bytes.
        scratch.put("a.pdf", B);
        let books = folder.scan().unwrap();
        assert_eq!(names(&books), [("aaaaaaaaaaaa", "a.pdf")]);
    }

    #[test]
    fn a_renamed_file_keeps_its_state_which_follows_the_new_name() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        let mut known = state("aaaaaaaaaaaa", "old.pdf", A);
        known.imported = 99;
        folder.write_state(&known).unwrap();
        scratch.put("new.pdf", A);
        let books = folder.scan().unwrap();
        assert_eq!(names(&books), [("aaaaaaaaaaaa", "new.pdf")]);
        assert_eq!(books[0].imported, 99);
        // The rename is stored.
        assert_eq!(folder.scan().unwrap(), books);
        let stored = fs::read(folder.state_file("aaaaaaaaaaaa")).unwrap();
        assert_eq!(BookState::decode(&stored).unwrap().file_name, "new.pdf");
    }

    #[test]
    fn a_file_renamed_and_edited_is_a_new_book_and_the_old_state_is_deleted() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        folder
            .write_state(&state("aaaaaaaaaaaa", "old.pdf", A))
            .unwrap();
        scratch.put("new.pdf", B);
        let books = folder.scan().unwrap();
        assert_eq!(books.len(), 1);
        assert_ne!(books[0].id, "aaaaaaaaaaaa");
        assert_eq!(books[0].file_name, "new.pdf");
        assert_eq!(books[0].sha256, sha(B));
        assert!(!folder.state_file("aaaaaaaaaaaa").exists());
    }

    #[test]
    fn a_file_without_state_is_a_new_book_with_its_own_stored_state() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        scratch.put("a.pdf", A);
        scratch.put("b.pdf", B);
        let books = folder.scan().unwrap();
        assert_eq!(books.len(), 2);
        assert_ne!(books[0].id, books[1].id);
        // Found again, not new again.
        let again = folder.scan().unwrap();
        assert_eq!(names(&again), names(&books));
        // A copy of a File is a Book of its own.
        scratch.put("a copy.pdf", A);
        assert_eq!(folder.scan().unwrap().len(), 3);
    }

    #[test]
    fn a_state_without_its_file_is_a_deleted_book() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        folder
            .write_state(&state("aaaaaaaaaaaa", "a.pdf", A))
            .unwrap();
        fs::write(folder.details("aaaaaaaaaaaa"), "1\n\n").unwrap();
        assert!(folder.scan().unwrap().is_empty());
        assert!(!folder.state_file("aaaaaaaaaaaa").exists());
        assert!(!folder.details("aaaaaaaaaaaa").exists());
    }

    #[test]
    fn states_of_one_file_merge_into_the_lowest_item_id_and_the_spare_file_goes() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        scratch.put("a.pdf", A);
        scratch.put("a 2.pdf", A);
        folder
            .write_state(&state("bbbbbbbbbbbb", "a 2.pdf", A))
            .unwrap();
        folder
            .write_state(&state("aaaaaaaaaaaa", "a.pdf", A))
            .unwrap();
        let books = folder.scan().unwrap();
        assert_eq!(names(&books), [("aaaaaaaaaaaa", "a.pdf")]);
        assert_eq!(visible(&scratch), ["a.pdf"]);
        assert!(!folder.state_file("bbbbbbbbbbbb").exists());
    }

    #[test]
    fn a_merge_keeps_the_only_file_there_is_and_never_deletes_a_changed_one() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        scratch.put("kept.pdf", A);
        folder
            .write_state(&state("aaaaaaaaaaaa", "gone.pdf", A))
            .unwrap();
        folder
            .write_state(&state("bbbbbbbbbbbb", "kept.pdf", A))
            .unwrap();
        assert_eq!(
            names(&folder.scan().unwrap()),
            [("aaaaaaaaaaaa", "kept.pdf")]
        );

        // The other File was edited since: it stays, as a Book of its own.
        scratch.put("edited.pdf", B);
        folder
            .write_state(&state("cccccccccccc", "edited.pdf", A))
            .unwrap();
        let books = folder.scan().unwrap();
        assert_eq!(books.len(), 2);
        assert_eq!(visible(&scratch), ["edited.pdf", "kept.pdf"]);
    }

    #[test]
    fn only_pdfs_and_epubs_are_books_and_hidden_files_and_folders_are_not() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        scratch.put("notes.txt", b"hello");
        scratch.put(".hidden.pdf", A);
        fs::create_dir(scratch.0.join("dir.pdf")).unwrap();
        assert!(folder.scan().unwrap().is_empty());
        assert_eq!(visible(&scratch), ["dir.pdf", "notes.txt"]);
    }

    #[test]
    fn an_unreadable_state_is_deleted_and_the_preferences_are_not_a_state() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        fs::write(folder.state_file("aaaaaaaaaaaa"), "garbage").unwrap();
        fs::write(folder.preferences(), "{}").unwrap();
        scratch.put("a.pdf", A);
        assert_eq!(folder.scan().unwrap().len(), 1);
        assert!(!folder.state_file("aaaaaaaaaaaa").exists());
        assert!(folder.preferences().exists());
    }

    #[test]
    fn a_taken_name_gets_2_then_3_before_the_extension() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        assert_eq!(folder.free_name("Emma.epub"), "Emma.epub");
        scratch.put("Emma.epub", A);
        assert_eq!(folder.free_name("Emma.epub"), "Emma 2.epub");
        scratch.put("Emma 2.epub", A);
        assert_eq!(folder.free_name("Emma.epub"), "Emma 3.epub");
        scratch.put("plain", A);
        assert_eq!(folder.free_name("plain"), "plain 2");
        assert_eq!(folder.free_name(".secret.pdf"), "secret.pdf");
        assert_eq!(folder.free_name("..."), "Book");
    }

    #[test]
    fn removing_a_book_takes_its_file_state_and_details() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        scratch.put("a.pdf", A);
        let books = folder.scan().unwrap();
        fs::write(folder.details(&books[0].id), "1\n\n").unwrap();
        folder.remove(&books[0]).unwrap();
        assert!(visible(&scratch).is_empty());
        assert!(folder.scan().unwrap().is_empty());
        assert_eq!(fs::read_dir(folder.state_dir()).unwrap().count(), 0);
        assert_eq!(fs::read_dir(folder.details_dir()).unwrap().count(), 0);
    }

    #[test]
    fn opening_discards_what_staging_holds() {
        let scratch = Scratch::new();
        let folder = scratch.folder();
        fs::write(folder.staging().join("half"), "x").unwrap();
        let folder = scratch.folder();
        assert_eq!(fs::read_dir(folder.staging()).unwrap().count(), 0);
    }
}
