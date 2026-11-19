//! The Notes store: a folder of `<id>.md` files, each with the front matter
//! `id`, `title` and `updated` and the Body as the file's body.

use pkm_base::clock::{iso8601, now_ms};
use pkm_base::files::{DiskFiles, Files};
use pkm_base::front_matter::Document;
use pkm_base::item_id::ItemId;
use pkm_base::markdown::row_preview;
use pkm_base::random::Entropy;
use std::fs;
use std::io;
use std::path::PathBuf;
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
            if self.files.exists(&self.path(&id)) {
                continue;
            }
            let mut file = Document::default();
            file.set_scalar("id", id.as_str());
            file.set_scalar(TITLE, "");
            file.set_scalar(UPDATED, iso8601(now_ms()));
            self.files.write(&self.path(&id), &file.render())?;
            return Ok(note_of(&file, &id));
        }
    }

    /// The Note `id`; `NotFound` if there is none.
    pub fn read(&self, id: &str) -> io::Result<Note> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let file = self.read_file(&id)?;
        Ok(note_of(&file, &id))
    }

    /// Sets the Title (line breaks become spaces) and Body of the Note `id`,
    /// both as given, and bumps `updated`. Saving what is saved already
    /// writes nothing, so that saving as the user types keeps Sync quiet.
    /// `NotFound` if there is no such Note.
    pub fn edit(&self, id: &str, title: &str, body: &str) -> io::Result<Note> {
        let id = ItemId::parse(id).ok_or(io::ErrorKind::NotFound)?;
        let mut file = self.read_file(&id)?;
        let before = file.render();
        file.set_scalar(TITLE, title.replace(['\r', '\n'], " "));
        file.body = body.to_owned();
        if file.render() != before {
            file.set_scalar(UPDATED, iso8601(now_ms()));
            self.files.write(&self.path(&id), &file.render())?;
        }
        Ok(note_of(&file, &id))
    }

    /// Deletes the Note `id`; one that is gone already is as wanted.
    pub fn delete(&self, id: &str) -> io::Result<()> {
        let Some(id) = ItemId::parse(id) else {
            return Ok(());
        };
        match self.files.remove(&self.path(&id)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
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
    /// extensions, hidden ones, `.md` without an `id` of their name) are
    /// skipped.
    pub fn list(&self) -> io::Result<Vec<Row>> {
        let mut rows = Vec::new();
        for listed in self.files.list(&self.root)? {
            let Some(id) = (listed.name.strip_suffix(".md")).and_then(ItemId::parse) else {
                continue;
            };
            let Ok(file) = self.read_file(&id) else {
                continue;
            };
            let note = note_of(&file, &id);
            rows.push(Row {
                preview: row_preview(&note.body),
                id: note.id,
                title: note.title,
                updated: file.scalar(UPDATED).unwrap_or_default().to_owned(),
            });
        }
        rows.sort_by(|a, b| b.updated.cmp(&a.updated).then_with(|| a.id.cmp(&b.id)));
        Ok(rows)
    }

    /// The file of the Note `id`; `NotFound` if it is none, which includes a
    /// file whose front matter names another `id`.
    fn read_file(&self, id: &ItemId) -> io::Result<Document> {
        let text = self.files.read(&self.path(id))?;
        Document::parse(&text)
            .filter(|file| file.scalar("id") == Some(id.as_str()))
            .ok_or_else(|| io::ErrorKind::NotFound.into())
    }

    fn path(&self, id: &ItemId) -> PathBuf {
        self.root.join(format!("{id}.md"))
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
    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            let id = ItemId::new(&mut Entropy::from_system());
            Self(
                std::env::temp_dir()
                    .join(format!("pkm-notes-{id}"))
                    .join("Notes"),
            )
        }

        fn store(&self) -> Notes {
            Notes::open(&self.0).unwrap()
        }

        fn write(&self, name: &str, text: &str) {
            fs::create_dir_all(&self.0).unwrap();
            fs::write(self.0.join(name), text).unwrap();
        }

        fn text(&self, id: &str) -> String {
            fs::read_to_string(self.0.join(format!("{id}.md"))).unwrap()
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
        fs::create_dir_all(scratch.0.join("Folder")).unwrap();
        let ids: Vec<_> = notes.list().unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(ids, [kept]);
    }
}
