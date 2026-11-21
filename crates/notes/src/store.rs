//! The Notes store: `<id>.md` files in one flat folder, each with the front
//! matter `id`, `title` and `updated` and the Body as the file's body.

mod images;

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

    /// Saves a new, empty Note under a fresh Item ID.
    pub fn create(&self) -> io::Result<Note> {
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
                .write(&self.root.join(file_name(&id)), &file.render())?;
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
                self.release_unshown(&image_targets(&file.body))
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

    /// Every Note, newest `updated` first. Files that are no Note (other
    /// extensions, hidden ones, `.md` without an `id` of their name) and
    /// anything in a subdirectory are skipped.
    pub fn list(&self) -> io::Result<Vec<Row>> {
        let mut rows: Vec<_> = (self.notes()?.into_iter())
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

    /// The Notes whose Title or Body has `query`, ignoring case
    /// and accents, anywhere in a word and in the text as typed. Those
    /// matching in the Title come first, then those matching only in the Body,
    /// each newest `updated` first. A blank query finds nothing. Reads every
    /// Note on each call.
    pub fn search(&self, query: &str) -> io::Result<Vec<Found>> {
        let Some(needle) = Needle::new(query) else {
            return Ok(Vec::new());
        };
        let mut hits = Vec::new();
        for (id, file) in self.notes()? {
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

    /// Every Note, with its file.
    fn notes(&self) -> io::Result<Vec<(ItemId, Document)>> {
        let mut notes = Vec::new();
        for listed in self.files.list(&self.root)? {
            let Some(id) = (listed.name.strip_suffix(".md")).and_then(ItemId::parse) else {
                continue;
            };
            if let Some(file) = self.read_file(&self.root.join(&listed.name), &id) {
                notes.push((id, file));
            }
        }
        Ok(notes)
    }

    /// The Note `id`: its Item ID, path and file; `NotFound` if there is none.
    fn find(&self, id: &str) -> io::Result<(ItemId, PathBuf, Document)> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let path = self.root.join(file_name(&id));
        match self.read_file(&path, &id) {
            Some(file) => Ok((id, path, file)),
            None => Err(io::ErrorKind::NotFound.into()),
        }
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

    fn ids(notes: &Notes) -> Vec<String> {
        notes.list().unwrap().into_iter().map(|r| r.id).collect()
    }

    fn note_file(id: &str, title: &str, updated: &str, body: &str) -> String {
        format!("---\nid: {id}\ntitle: {title}\nupdated: {updated}\n---\n{body}")
    }

    #[test]
    fn a_created_note_is_empty_and_kept_as_a_file_named_by_its_id() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let note = notes.create().unwrap();
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
        let id = notes.create().unwrap().id;
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
        let id = notes.create().unwrap().id;
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
        assert_eq!(notes.list().unwrap()[0].updated, "2020-01-01T00:00:00Z");
        notes.edit(id, "new", "x\n").unwrap();
        assert!(notes.list().unwrap()[0].updated.as_str() > "2020-01-01T00:00:00Z");
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
        let rows = notes.list().unwrap();
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
        let empty = notes.create().unwrap().id;
        let blank = notes.create().unwrap().id;
        notes.edit(&blank, " ", "\n").unwrap();
        let titled = notes.create().unwrap().id;
        notes.edit(&titled, "T", "").unwrap();
        let written = notes.create().unwrap().id;
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
        let id = notes.create().unwrap().id;
        notes.delete(&id).unwrap();
        assert!(notes.list().unwrap().is_empty());
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
        let kept = notes.create().unwrap().id;
        let note = note_file("cccccccccccc", "x", "2020-01-01T00:00:00Z", "");
        scratch.write("cccccccccccc.txt", &note);
        scratch.write(".cccccccccccc.md", &note);
        scratch.write("dddddddddddd.md", "---\ntitle: no id\n---\n");
        scratch.write("eeeeeeeeeeee.md", "just text");
        scratch.write("ffffffffffff.md", &note);
        assert_eq!(ids(&notes), [kept]);
    }

    #[test]
    fn notes_in_a_subdirectory_are_ignored_and_never_deleted() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        let top = notes.create().unwrap().id;
        let note = note_file("aaaaaaaaaaaa", "T", "2020-01-01T00:00:00Z", "cafe\n");
        scratch.write("Work/aaaaaaaaaaaa.md", &note);
        assert_eq!(ids(&notes), [top]);
        assert!(notes.search("cafe").unwrap().is_empty());
        assert_eq!(
            notes.read("aaaaaaaaaaaa").unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
        notes.delete("aaaaaaaaaaaa").unwrap();
        assert!(scratch.exists_file("Work/aaaaaaaaaaaa.md"));
    }

    #[test]
    fn search_finds_notes_with_a_snippet() {
        let scratch = Scratch::new();
        let notes = scratch.store();
        scratch.write(
            "aaaaaaaaaaaa.md",
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
