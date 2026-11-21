//! The Notes store: `<id>.md` files in nested folders, each with the front
//! matter `id`, `title` and `updated` and the Body as the file's body.

mod images;

use crate::folder::{Counts, Folder, FolderError, check_name, is_visible_name};
use pkm_base::clock::{iso8601, now_ms};
use pkm_base::files::{DiskFiles, Files};
use pkm_base::front_matter::Document;
use pkm_base::item_id::ItemId;
use pkm_base::markdown::{image_targets, row_preview};
use pkm_base::random::Entropy;
use pkm_search::{Needle, Snippet, start_of};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TITLE: &str = "title";
const UPDATED: &str = "updated";

/// A Note: its Title (one line, maybe empty) and Body, both as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
}

/// A Note as one row of the list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub title: String,
    /// The line of the Body a row shows (`row_preview`); format it like a
    /// Title. Shown when the Title is empty.
    pub preview: String,
    /// When it last changed, as UTC ISO 8601.
    pub updated: String,
}

/// A Note that Search found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    pub id: String,
    pub title: String,
    /// The path of the Folder it is in; empty for the top level.
    pub folder: String,
    /// Around the first match in the Body; the Body's start, with no match
    /// marked, where only the Title matched.
    pub snippet: Snippet,
    pub title_matched: bool,
}

/// The Notes in one folder (`Notes` in the app's storage).
#[derive(Debug)]
pub struct Notes {
    files: Arc<dyn Files>,
    root: PathBuf,
}

impl Notes {
    /// The folder's name, in the app's storage and in the cloud.
    pub const FOLDER: &'static str = "Notes";

    /// Opens the Notes in `root`, creating the folder if needed.
    pub fn open(root: impl Into<PathBuf>) -> io::Result<Self> {
        let root = root.into();
        fs::create_dir_all(&root)?;
        Ok(Self::open_in(Arc::new(DiskFiles), root))
    }

    /// The Notes in `root` on any `files`, which must hold the folder `root`
    /// (an empty one is none).
    pub fn open_in(files: Arc<dyn Files>, root: impl Into<PathBuf>) -> Self {
        Self {
            files,
            root: root.into(),
        }
    }

    /// Saves a new, empty Note under a fresh Item ID in `folder` (a path,
    /// empty for the top level); `NotFound` if there is no such Folder.
    pub fn create(&self, folder: &str) -> io::Result<Note> {
        let dir = self.dir(folder)?;
        let mut entropy = Entropy::from_system();
        loop {
            let id = ItemId::new(&mut entropy);
            if self.find(id.as_str()).is_ok() {
                continue;
            }
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar(TITLE, "");
            file.set_scalar(UPDATED, iso8601(now_ms()));
            self.files
                .write(&dir.join(file_name(&id)), &file.render())?;
            return Ok(note_of(&file, &id));
        }
    }

    /// The Note `id`, wherever it is; `NotFound` if there is none.
    pub fn read(&self, id: &str) -> io::Result<Note> {
        let (id, _, file) = self.find(id)?;
        Ok(note_of(&file, &id))
    }

    /// Sets the Title (line breaks become spaces) and Body of the Note `id`,
    /// both as given, and bumps `updated`. Saving what is saved already
    /// writes nothing, so that saving as the user types keeps Sync quiet.
    /// `NotFound` if there is no such Note.
    pub fn edit(&self, id: &str, title: &str, body: &str) -> io::Result<Note> {
        let (id, path, mut file) = self.find(id)?;
        let before = file.render();
        file.set_scalar(TITLE, title.replace(['\r', '\n'], " "));
        file.body = body.to_owned();
        if file.render() != before {
            file.set_scalar(UPDATED, iso8601(now_ms()));
            self.files.write(&path, &file.render())?;
        }
        Ok(note_of(&file, &id))
    }

    /// Deletes the Note `id` and the Images only it shows; one that is gone
    /// already is as wanted.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        match self.find(id) {
            Ok((_, path, file)) => {
                self.files.remove(&path)?;
                self.release_beside(&path, &image_targets(&file.body))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Deletes the Note `id` if its Title and Body are empty (only whitespace
    /// counts as empty), for a client leaving it.
    pub fn discard_if_empty(&self, id: &str) -> io::Result<()> {
        match self.read(id) {
            Ok(note) if note.title.trim().is_empty() && note.body.trim().is_empty() => {
                self.delete(id)
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// The Notes directly in `folder`, newest `updated` first. Files that
    /// are no Note (other extensions, hidden ones, `.md` without an `id` of
    /// their name) are skipped.
    pub fn list(&self, folder: &str) -> io::Result<Vec<Row>> {
        let mut rows: Vec<_> = (self.notes_in(&self.dir(folder)?)?.into_iter())
            .map(|(id, file)| {
                let note = note_of(&file, &id);
                Row {
                    preview: row_preview(&note.body),
                    id: note.id,
                    title: note.title,
                    updated: file.scalar(UPDATED).unwrap_or_default().to_owned(),
                }
            })
            .collect();
        rows.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        Ok(rows)
    }

    /// The Notes in any Folder whose Title or Body has `query`, ignoring case
    /// and accents, anywhere in a word and in the text as typed. Those
    /// matching in the Title come first, then those matching only in the Body,
    /// each newest `updated` first. A blank query finds nothing. Reads every
    /// Note on each call.
    pub fn search(&self, query: &str) -> io::Result<Vec<Found>> {
        let Some(needle) = Needle::new(query) else {
            return Ok(Vec::new());
        };
        let mut hits = Vec::new();
        for (folder, id, file) in self.all_notes()? {
            let note = note_of(&file, &id);
            let title_matched = needle.find(&note.title).is_some();
            let snippet = match needle.find(&note.body) {
                Some(snippet) => snippet,
                None if title_matched => start_of(&note.body),
                None => continue,
            };
            let updated = file.scalar(UPDATED).unwrap_or_default().to_owned();
            let found = Found {
                id: note.id,
                title: note.title,
                folder,
                snippet,
                title_matched,
            };
            hits.push((updated, found));
        }
        hits.sort_by(|(a_updated, a), (b_updated, b)| {
            (b.title_matched.cmp(&a.title_matched))
                .then_with(|| b_updated.cmp(a_updated))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(hits.into_iter().map(|(_, found)| found).collect())
    }

    /// The Folders at the top level, each with the Folders in it, by name
    /// (ignoring case).
    pub fn folders(&self) -> io::Result<Vec<Folder>> {
        self.folders_in(&self.root, "")
    }

    /// Makes the Folder `name` in `parent` (a path, empty for the top level)
    /// and returns its path.
    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String, FolderError> {
        let name = check_name(name)?;
        let dir = self.dir(parent)?;
        self.check_free(&dir, name, None)?;
        self.files.make_folder(&dir.join(name))?;
        Ok(join(parent, name))
    }

    /// Renames the Folder at `path` and returns its new path.
    pub fn rename_folder(&self, path: &str, name: &str) -> Result<String, FolderError> {
        let name = check_name(name)?;
        let dir = self.dir(path)?;
        let (parent, old) = path.rsplit_once('/').unwrap_or(("", path));
        if old.is_empty() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let parent_dir = self.dir(parent)?;
        self.check_free(&parent_dir, name, Some(old))?;
        if name != old {
            self.files.rename(&dir, &parent_dir.join(name))?;
        }
        Ok(join(parent, name))
    }

    /// Moves the Note `id` into the Folder `folder` (empty for the top level),
    /// keeping its Item ID, and bumps `updated`. The Images only it shows go
    /// with it, those another Note beside it shows stay. `NotFound` if there is no
    /// such Note or Folder; Moving it where it is changes nothing.
    pub fn move_note(&self, id: &str, folder: &str) -> io::Result<()> {
        let (id, from, mut file) = self.find(id)?;
        let to = self.dir(folder)?.join(file_name(&id));
        if to == from {
            return Ok(());
        }
        self.files.rename(&from, &to)?;
        file.set_scalar(UPDATED, iso8601(now_ms()));
        self.files.write(&to, &file.render())?;
        self.move_images(&id, &file.body, &from, &to)
    }

    /// Moves the Folder at `path` with everything in it into the Folder
    /// `into` (empty for the top level) and returns its new path. Refused
    /// with `Inside` into itself or a Folder in it, and with `Taken` if
    /// `into` has a Folder of its name. Moving it where it is changes nothing.
    pub fn move_folder(&self, path: &str, into: &str) -> Result<String, FolderError> {
        let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
        if name.is_empty() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput).into());
        }
        let from = self.dir(path)?;
        let dest = self.dir(into)?;
        if into == path || into.strip_prefix(path).is_some_and(|r| r.starts_with('/')) {
            return Err(FolderError::Inside);
        }
        if parent == into {
            return Ok(path.to_owned());
        }
        self.check_free(&dest, name, None)?;
        self.files.rename(&from, &dest.join(name))?;
        Ok(join(into, name))
    }

    /// Deletes the Folder at `path` with everything in it; one that is gone
    /// already is as wanted.
    pub fn delete_folder(&self, path: &str) -> io::Result<()> {
        if path.is_empty() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        match self.dir(path) {
            Ok(dir) => self.files.remove_folder(&dir),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// The Notes and Folders in the Folder at `path`, all the way down: what
    /// Deleting it would Delete.
    pub fn count(&self, path: &str) -> io::Result<Counts> {
        let mut counts = Counts {
            notes: 0,
            folders: 0,
        };
        let mut pending = vec![self.dir(path)?];
        while let Some(dir) = pending.pop() {
            counts.notes += self.notes_in(&dir)?.len() as u32;
            let inner = self.subfolders(&dir)?;
            counts.folders += inner.len() as u32;
            pending.extend(inner.iter().map(|name| dir.join(name)));
        }
        Ok(counts)
    }

    /// The directory of the Folder `path`; `NotFound` if there is none, or
    /// `path` names something hidden or climbing up.
    fn dir(&self, path: &str) -> io::Result<PathBuf> {
        let mut dir = self.root.clone();
        if path.is_empty() {
            return Ok(dir);
        }
        for name in path.split('/') {
            if !is_visible_name(name) || !self.files.list_folders(&dir)?.iter().any(|n| n == name) {
                return Err(io::ErrorKind::NotFound.into());
            }
            dir.push(name);
        }
        Ok(dir)
    }

    /// The names of the visible Folders directly in `dir`.
    fn subfolders(&self, dir: &Path) -> io::Result<Vec<String>> {
        let mut names = self.files.list_folders(dir)?;
        names.retain(|name| is_visible_name(name));
        Ok(names)
    }

    fn folders_in(&self, dir: &Path, path: &str) -> io::Result<Vec<Folder>> {
        let mut names = self.subfolders(dir)?;
        names.sort_by_cached_key(|name| (name.to_lowercase(), name.clone()));
        names
            .into_iter()
            .map(|name| {
                let path = join(path, &name);
                Ok(Folder {
                    folders: self.folders_in(&dir.join(&name), &path)?,
                    name,
                    path,
                })
            })
            .collect()
    }

    /// `Taken` if `dir` holds a Folder `name` in other case or not, other
    /// than `except` itself.
    fn check_free(&self, dir: &Path, name: &str, except: Option<&str>) -> Result<(), FolderError> {
        let lower = name.to_lowercase();
        let taken = (self.subfolders(dir)?.iter())
            .any(|other| Some(other.as_str()) != except && other.to_lowercase() == lower);
        if taken {
            Err(FolderError::Taken)
        } else {
            Ok(())
        }
    }

    /// Every Note directly in `dir`, with its file.
    fn notes_in(&self, dir: &Path) -> io::Result<Vec<(ItemId, Document)>> {
        let mut notes = Vec::new();
        for listed in self.files.list(dir)? {
            let Some(id) = (listed.name.strip_suffix(".md")).and_then(ItemId::parse) else {
                continue;
            };
            if let Some(file) = self.read_file(&dir.join(&listed.name), &id) {
                notes.push((id, file));
            }
        }
        Ok(notes)
    }

    /// Every Note in any Folder, with its Folder's path and file.
    fn all_notes(&self) -> io::Result<Vec<(String, ItemId, Document)>> {
        let mut all = Vec::new();
        let mut pending = vec![(String::new(), self.root.clone())];
        while let Some((path, dir)) = pending.pop() {
            all.extend(
                (self.notes_in(&dir)?.into_iter()).map(|(id, file)| (path.clone(), id, file)),
            );
            for name in self.subfolders(&dir)? {
                pending.push((join(&path, &name), dir.join(&name)));
            }
        }
        Ok(all)
    }

    /// The Note `id`, found in whichever Folder it is: its Item ID, path and
    /// file; `NotFound` if there is none.
    fn find(&self, id: &str) -> io::Result<(ItemId, PathBuf, Document)> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let mut pending = vec![self.root.clone()];
        while let Some(dir) = pending.pop() {
            let path = dir.join(file_name(&id));
            if let Some(file) = self.read_file(&path, &id) {
                return Ok((id, path, file));
            }
            pending.extend(self.subfolders(&dir)?.iter().map(|name| dir.join(name)));
        }
        Err(io::ErrorKind::NotFound.into())
    }

    /// The Note file at `path`, none if it is none, which includes a file
    /// whose front matter names another `id`.
    fn read_file(&self, path: &Path, id: &ItemId) -> Option<Document> {
        let text = self.files.read(path).ok()?;
        Document::parse(&text).filter(|file| file.scalar("id") == Some(id.as_str()))
    }
}

fn file_name(id: &ItemId) -> String {
    format!("{id}.md")
}

fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    }
}

fn note_of(file: &Document, id: &ItemId) -> Note {
    Note {
        id: id.as_str().to_owned(),
        title: file.scalar(TITLE).unwrap_or_default().to_owned(),
        body: file.body.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Notes folder in a temporary directory, removed afterwards.
    pub(super) struct Scratch(pub(super) PathBuf);

    impl Scratch {
        pub(super) fn new() -> Self {
            let id = ItemId::new(&mut Entropy::from_system());
            Self(
                std::env::temp_dir()
                    .join(format!("pkm-notes-{id}"))
                    .join("Notes"),
            )
        }

        pub(super) fn store(&self) -> Notes {
            Notes::open(&self.0).unwrap()
        }

        fn write(&self, name: &str, text: &str) {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }

        fn text(&self, id: &str) -> String {
            fs::read_to_string(self.0.join(format!("{id}.md"))).unwrap()
        }

        pub(super) fn exists_file(&self, name: &str) -> bool {
            self.0.join(name).exists()
        }

        fn exists(&self, id: &str) -> bool {
            self.0.join(format!("{id}.md")).exists()
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            drop(fs::remove_dir_all(self.0.parent().unwrap()));
        }
    }

    fn note_file(id: &str, title: &str, updated: &str, body: &str) -> String {
        format!("---\nid: {id}\ntitle: {title}\nupdated: {updated}\n---\n{body}")
    }

    #[test]
    fn a_created_note_is_empty_and_kept_as_a_file_named_by_its_id() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let note = notes.create("").unwrap();
        assert_eq!((note.title.as_str(), note.body.as_str()), ("", ""));
        assert_eq!(notes.read(&note.id).unwrap(), note);
        let text = scratch.text(&note.id);
        assert!(text.starts_with(&format!("---\nid: {}\ntitle: \"\"\nupdated: 20", note.id)));
        assert!(text.ends_with("Z\n---\n"));
    }

    #[test]
    fn an_edit_keeps_title_and_body_as_typed_in_the_file() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = notes.create("").unwrap().id;
        let body = "# Heading\n\n  - **bold**: x \n";
        let edited = notes.edit(&id, "A *Title*: here ", body).unwrap();
        assert_eq!(edited.title, "A *Title*: here ");
        assert_eq!(edited.body, body);
        assert_eq!(notes.read(&id).unwrap(), edited);
        assert!(scratch.text(&id).ends_with(&format!("---\n{body}")));
    }

    #[test]
    fn a_title_stays_one_line() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = notes.create("").unwrap().id;
        assert_eq!(notes.edit(&id, "a\nb\r\nc", "").unwrap().title, "a b  c");
    }

    #[test]
    fn an_edit_sets_updated_and_saving_the_same_writes_nothing() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = "aaaaaaaaaaaa";
        scratch.write(
            "aaaaaaaaaaaa.md",
            &note_file(id, "old", "2020-01-01T00:00:00Z", "x\n"),
        );
        notes.edit(id, "old", "x\n").unwrap();
        assert_eq!(notes.list("").unwrap()[0].updated, "2020-01-01T00:00:00Z");
        notes.edit(id, "new", "x\n").unwrap();
        assert!(notes.list("").unwrap()[0].updated.as_str() > "2020-01-01T00:00:00Z");
    }

    #[test]
    fn notes_are_listed_newest_updated_first_with_title_and_preview() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        scratch.write(
            "aaaaaaaaaaaa.md",
            &note_file("aaaaaaaaaaaa", "Old", "2020-01-01T00:00:00Z", ""),
        );
        scratch.write(
            "bbbbbbbbbbbb.md",
            &note_file(
                "bbbbbbbbbbbb",
                "\"\"",
                "2021-01-01T00:00:00Z",
                "\n## First line\nmore\n",
            ),
        );
        let rows = notes.list("").unwrap();
        let shown: Vec<_> = rows
            .iter()
            .map(|r| (&r.id[..1], &r.title[..], &r.preview[..]))
            .collect();
        assert_eq!(shown, [("b", "", "First line"), ("a", "Old", "")]);
        assert_eq!(rows[0].updated, "2021-01-01T00:00:00Z");
    }

    #[test]
    fn a_note_with_nothing_in_it_is_discarded() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let empty = notes.create("").unwrap().id;
        let blank = notes.create("").unwrap().id;
        notes.edit(&blank, " ", "\n").unwrap();
        let titled = notes.create("").unwrap().id;
        notes.edit(&titled, "T", "").unwrap();
        let written = notes.create("").unwrap().id;
        notes.edit(&written, "", "text").unwrap();
        for id in [&empty, &blank, &titled, &written] {
            notes.discard_if_empty(id).unwrap();
        }
        assert!(!scratch.exists(&empty) && !scratch.exists(&blank));
        assert!(scratch.exists(&titled) && scratch.exists(&written));
        notes.discard_if_empty(&empty).unwrap();
    }

    #[test]
    fn a_deleted_note_is_gone() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = notes.create("").unwrap().id;
        notes.delete(&id).unwrap();
        assert!(notes.list("").unwrap().is_empty());
        assert_eq!(notes.read(&id).unwrap_err().kind(), io::ErrorKind::NotFound);
        assert_eq!(
            notes.edit(&id, "", "").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        notes.delete(&id).unwrap();
    }

    #[test]
    fn files_that_are_no_note_are_skipped() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let kept = notes.create("").unwrap().id;
        let note = note_file("cccccccccccc", "x", "2020-01-01T00:00:00Z", "");
        scratch.write("cccccccccccc.txt", &note);
        scratch.write(".cccccccccccc.md", &note);
        scratch.write("dddddddddddd.md", "---\ntitle: no id\n---\n");
        scratch.write("eeeeeeeeeeee.md", "just text");
        scratch.write("ffffffffffff.md", &note);
        fs::create_dir_all(scratch.0.join("Folder")).unwrap();
        let ids: Vec<_> = notes.list("").unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, [kept]);
    }

    fn names(folders: &[Folder]) -> Vec<&str> {
        folders.iter().map(|f| f.name.as_str()).collect()
    }

    #[test]
    fn folders_nest_and_empty_ones_stay() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let work = notes.create_folder("", "Work").unwrap();
        let inner = notes.create_folder(&work, "Plans").unwrap();
        assert_eq!(inner, "Work/Plans");
        notes.create_folder("", "archive").unwrap();
        notes.create_folder("", "Books").unwrap();
        let tree = notes.folders().unwrap();
        assert_eq!(names(&tree), ["archive", "Books", "Work"]);
        assert_eq!(names(&tree[2].folders), ["Plans"]);
        assert_eq!(tree[2].folders[0].path, "Work/Plans");
        assert!(scratch.0.join("Work/Plans").is_dir());
        assert!(matches!(
            notes.create_folder("Nope", "x"),
            Err(FolderError::Io(e)) if e.kind() == io::ErrorKind::NotFound
        ));
    }

    #[test]
    fn folder_names_are_refused_with_a_reason() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "Work").unwrap();
        assert!(matches!(
            notes.create_folder("", " "),
            Err(FolderError::Empty)
        ));
        assert!(matches!(
            notes.create_folder("", "a/b"),
            Err(FolderError::HasSlash)
        ));
        assert!(matches!(
            notes.create_folder("", ".x"),
            Err(FolderError::Hidden)
        ));
        assert!(matches!(
            notes.create_folder("", "WORK"),
            Err(FolderError::Taken)
        ));
        notes.create_folder("Work", "work").unwrap();
        notes.create_folder("", "Home").unwrap();
        assert!(matches!(
            notes.rename_folder("Home", "wOrK"),
            Err(FolderError::Taken)
        ));
        assert_eq!(notes.rename_folder("Home", "HOME").unwrap(), "HOME");
        assert_eq!(
            notes.rename_folder("Home", "").unwrap_err().to_string(),
            "The name is empty."
        );
    }

    #[test]
    fn a_renamed_folder_keeps_its_notes_which_are_found_by_id() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let work = notes.create_folder("", "Work").unwrap();
        let id = notes.create(&work).unwrap().id;
        notes.edit(&id, "Plan", "x").unwrap();
        assert_eq!(notes.list("").unwrap().len(), 0);
        let path = notes.rename_folder(&work, "Job").unwrap();
        assert_eq!(path, "Job");
        assert_eq!(notes.list("Job").unwrap()[0].id, id);
        assert_eq!(notes.read(&id).unwrap().title, "Plan");
        assert!(matches!(notes.list("Work"), Err(e) if e.kind() == io::ErrorKind::NotFound));
    }

    #[test]
    fn a_moved_note_keeps_its_id_and_its_updated_is_set() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let id = "aaaaaaaaaaaa";
        scratch.write(
            "aaaaaaaaaaaa.md",
            &note_file(id, "T", "2020-01-01T00:00:00Z", "body\n"),
        );
        notes.create_folder("", "A").unwrap();
        notes.create_folder("A", "B").unwrap();
        notes.move_note(id, "A/B").unwrap();
        assert!(notes.list("").unwrap().is_empty());
        let rows = notes.list("A/B").unwrap();
        assert_eq!((rows[0].id.as_str(), rows[0].title.as_str()), (id, "T"));
        assert!(rows[0].updated.as_str() > "2020-01-01T00:00:00Z");
        assert_eq!(notes.read(id).unwrap().body, "body\n");
        notes.move_note(id, "").unwrap();
        assert_eq!(notes.list("").unwrap().len(), 1);
        assert!(notes.move_note(id, "Nope").is_err());
        assert!(notes.move_note("bbbbbbbbbbbb", "A").is_err());
    }

    #[test]
    fn a_moved_folder_takes_everything_in_it() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "A").unwrap();
        notes.create_folder("A", "Inner").unwrap();
        notes.create_folder("", "B").unwrap();
        let id = notes.create("A/Inner").unwrap().id;
        assert_eq!(notes.move_folder("A", "B").unwrap(), "B/A");
        assert_eq!(notes.list("B/A/Inner").unwrap()[0].id, id);
        assert_eq!(names(&notes.folders().unwrap()), ["B"]);
        assert_eq!(notes.move_folder("B/A/Inner", "").unwrap(), "Inner");
        assert_eq!(notes.move_folder("Inner", "").unwrap(), "Inner");
        assert_eq!(notes.list("Inner").unwrap()[0].id, id);
    }

    #[test]
    fn a_folder_is_refused_into_itself_a_descendant_or_a_name_clash() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "A").unwrap();
        notes.create_folder("A", "B").unwrap();
        notes.create_folder("", "AB").unwrap();
        notes.create_folder("", "b").unwrap();
        assert!(matches!(
            notes.move_folder("A", "A"),
            Err(FolderError::Inside)
        ));
        assert!(matches!(
            notes.move_folder("A", "A/B"),
            Err(FolderError::Inside)
        ));
        assert!(matches!(
            notes.move_folder("A/B", ""),
            Err(FolderError::Taken)
        ));
        assert_eq!(notes.move_folder("A", "AB").unwrap(), "AB/A");
        assert!(notes.move_folder("", "AB").is_err());
        assert!(notes.move_folder("Nope", "").is_err());
    }

    #[test]
    fn a_folder_lists_only_its_own_notes() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let top = notes.create("").unwrap().id;
        notes.create_folder("", "A").unwrap();
        let inner = notes.create("A").unwrap().id;
        let ids = |folder| -> Vec<String> {
            notes
                .list(folder)
                .unwrap()
                .into_iter()
                .map(|r| r.id)
                .collect()
        };
        assert_eq!((ids(""), ids("A")), (vec![top], vec![inner]));
        for bad in ["../x", ".h", "A//B", "A/.."] {
            assert!(notes.list(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn deleting_a_folder_deletes_everything_in_it_and_counts_say_what() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        notes.create_folder("", "A").unwrap();
        notes.create_folder("A", "B").unwrap();
        notes.create_folder("A/B", "C").unwrap();
        let a = notes.create("A").unwrap().id;
        let b = notes.create("A/B").unwrap().id;
        let kept = notes.create("").unwrap().id;
        scratch.write("A/B/photo.png", "not a note");
        assert_eq!(
            notes.count("A").unwrap(),
            Counts {
                notes: 2,
                folders: 2
            }
        );
        assert_eq!(
            notes.count("A/B/C").unwrap(),
            Counts {
                notes: 0,
                folders: 0
            }
        );
        notes.delete_folder("A").unwrap();
        for id in [a, b] {
            assert_eq!(notes.read(&id).unwrap_err().kind(), io::ErrorKind::NotFound);
        }
        assert!(!scratch.0.join("A").exists());
        assert!(notes.folders().unwrap().is_empty());
        assert!(scratch.exists(&kept));
        notes.delete_folder("A").unwrap();
        assert!(notes.delete_folder("").is_err());
    }

    #[test]
    fn search_finds_notes_in_any_folder_with_their_folder_and_snippet() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        scratch.write(
            "a/b/aaaaaaaaaaaa.md",
            &note_file(
                "aaaaaaaaaaaa",
                "T",
                "2020-01-01T00:00:00Z",
                "one\n**Café** two\n",
            ),
        );
        scratch.write(
            ".hidden/bbbbbbbbbbbb.md",
            &note_file("bbbbbbbbbbbb", "T", "2020-01-01T00:00:00Z", "cafe\n"),
        );
        let found = notes.search("CAFE").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].folder, "a/b");
        assert!(!found[0].title_matched);
        assert_eq!(found[0].snippet.text, "one **Café** two");
        assert_eq!(found[0].snippet.matched, 6..10);
        assert!(notes.search("  ").unwrap().is_empty());
    }

    #[test]
    fn search_lists_title_matches_first_then_body_matches_each_newest_first() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let file = |id: &str, title: &str, updated: &str, body: &str| {
            scratch.write(&format!("{id}.md"), &note_file(id, title, updated, body));
        };
        file("aaaaaaaaaaaa", "x", "2022-01-01T00:00:00Z", "tea\n");
        file("bbbbbbbbbbbb", "Tea", "2020-01-01T00:00:00Z", "");
        file("cccccccccccc", "x", "2021-01-01T00:00:00Z", "tea\n");
        file(
            "dddddddddddd",
            "Teatime",
            "2021-01-01T00:00:00Z",
            "first\ntea\n",
        );
        let ids: Vec<_> = notes
            .search("tea")
            .unwrap()
            .into_iter()
            .map(|f| f.id)
            .collect();
        assert_eq!(
            ids,
            [
                "dddddddddddd",
                "bbbbbbbbbbbb",
                "aaaaaaaaaaaa",
                "cccccccccccc"
            ]
        );
        let title_only = &notes.search("tea").unwrap()[1];
        assert!(title_only.title_matched);
        assert_eq!(title_only.snippet.matched, 0..0);
    }

    #[test]
    fn a_search_match_is_counted_in_utf16_units_after_an_emoji() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        scratch.write(
            "aaaaaaaaaaaa.md",
            &note_file("aaaaaaaaaaaa", "T", "2020-01-01T00:00:00Z", "😀 tea\n"),
        );
        assert_eq!(notes.search("tea").unwrap()[0].snippet.matched, 3..6);
    }
}
