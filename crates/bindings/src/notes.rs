//! The Notes surface of the UniFFI library (ADR-0007): the Notes store. The
//! logic lives in `pkm-notes`.

use pkm_notes::{Found, Note, Notes, Row};

/// Why the Notes couldn't be read or saved.
#[derive(Debug, uniffi::Error)]
#[uniffi(flat_error)]
pub enum NotesError {
    Failed(String),
}

impl std::fmt::Display for NotesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self::Failed(message) = self;
        f.write_str(message)
    }
}

impl From<std::io::Error> for NotesError {
    fn from(e: std::io::Error) -> Self {
        Self::Failed(e.to_string())
    }
}

/// A Note's Title and Body as typed, see `pkm_notes::Note`.
#[uniffi::remote(Record)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
    /// In the order they were added.
    pub tags: Vec<String>,
}

/// A Note as one row of the list, see `pkm_notes::Row` (named apart from
/// the Tasks' rows).
#[derive(uniffi::Record)]
pub struct ListedNote {
    pub id: String,
    pub title: String,
    /// The Body's line to show when the Title is empty; format it like a Title.
    pub preview: String,
    /// UTC ISO 8601.
    pub updated: String,
    pub tags: Vec<String>,
}

impl From<Row> for ListedNote {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            title: row.title,
            preview: row.preview,
            updated: row.updated,
            tags: row.tags,
        }
    }
}

/// A Note that Search found, see `pkm_notes::Found`.
#[derive(uniffi::Record)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    /// Around the match in the Body, as typed.
    pub snippet: String,
    /// The match in `snippet`, in UTF-16 units: `matched_start` up to `matched_end`.
    pub matched_start: u32,
    pub matched_end: u32,
    pub title_matched: bool,
}

impl From<Found> for SearchResult {
    fn from(found: Found) -> Self {
        Self {
            id: found.id,
            title: found.title,
            snippet: found.snippet.text,
            matched_start: found.snippet.matched.start as u32,
            matched_end: found.snippet.matched.end as u32,
            title_matched: found.title_matched,
        }
    }
}

/// The Notes in a folder, see `pkm_notes::Notes`.
#[derive(uniffi::Object)]
pub struct NoteStore(Notes);

#[uniffi::export]
impl NoteStore {
    /// Opens the Notes in the folder `root`, creating it if needed.
    #[uniffi::constructor]
    pub fn open(root: String) -> Result<Self, NotesError> {
        Ok(Self(Notes::open(root)?))
    }

    /// Saves a new Note, empty but for the valid `tags` to start with.
    pub fn create(&self, tags: Vec<String>) -> Result<Note, NotesError> {
        Ok(self.0.create(&tags)?)
    }

    pub fn read(&self, id: String) -> Result<Note, NotesError> {
        Ok(self.0.read(&id)?)
    }

    /// Sets the Title and Body as typed and bumps `updated`.
    pub fn edit(&self, id: String, title: String, body: String) -> Result<Note, NotesError> {
        Ok(self.0.edit(&id, &title, &body)?)
    }

    /// Puts the Tag on the Note, unless it has it; a new Tag joins the Tags file.
    pub fn add_tag(&self, id: String, tag: String) -> Result<Note, NotesError> {
        Ok(self.0.add_tag(&id, &tag)?)
    }

    /// Takes the Tag off the Note; it stays known.
    pub fn remove_tag(&self, id: String, tag: String) -> Result<Note, NotesError> {
        Ok(self.0.remove_tag(&id, &tag)?)
    }

    pub fn delete(&self, id: String) -> Result<(), NotesError> {
        Ok(self.0.delete(&id)?)
    }

    /// Keeps `bytes`, a PNG, JPEG, GIF or WebP, beside the Note; the reference for its Body.
    pub fn add_image(&self, id: String, bytes: Vec<u8>) -> Result<String, NotesError> {
        Ok(self.0.add_image(&id, &bytes)?)
    }

    /// The bytes of the Image `reference` names beside the Note.
    pub fn read_image(&self, id: String, reference: String) -> Result<Vec<u8>, NotesError> {
        Ok(self.0.read_image(&id, &reference)?)
    }

    /// Deletes those of the `references` beside the Note that no Note shows any more; for leaving it.
    pub fn release_images(&self, id: String, references: Vec<String>) -> Result<(), NotesError> {
        Ok(self.0.release_images(&id, &references)?)
    }

    /// Deletes the Note if its Title and Body are empty; for leaving it.
    pub fn discard_if_empty(&self, id: String) -> Result<(), NotesError> {
        Ok(self.0.discard_if_empty(&id)?)
    }

    /// Every Note, newest `updated` first.
    pub fn notes(&self) -> Result<Vec<ListedNote>, NotesError> {
        Ok(self.0.list()?.into_iter().map(Into::into).collect())
    }

    /// The Notes carrying exactly the Tag, newest `updated` first.
    pub fn notes_tagged(&self, tag: String) -> Result<Vec<ListedNote>, NotesError> {
        Ok(self
            .0
            .list_tagged(&tag)?
            .into_iter()
            .map(Into::into)
            .collect())
    }

    /// The Tags some Note carries, alphabetically.
    pub fn tags_in_use(&self) -> Result<Vec<String>, NotesError> {
        Ok(self.0.tags_in_use()?)
    }

    /// The Notes with `query` in the Title or Body, Title matches first, each newest first.
    pub fn search(&self, query: String) -> Result<Vec<SearchResult>, NotesError> {
        Ok(self.0.search(&query)?.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_written_through_the_bindings_is_listed() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-notes-{}", std::process::id()));
        let store = NoteStore::open(dir.join("Notes").to_string_lossy().into_owned()).unwrap();
        let note = store.create(vec!["work".into()]).unwrap();
        store.add_tag(note.id.clone(), "home".into()).unwrap();
        store
            .edit(note.id.clone(), "Title".into(), "- first\n".into())
            .unwrap();
        let rows = store.notes().unwrap();
        assert_eq!(
            (rows[0].title.as_str(), rows[0].preview.as_str()),
            ("Title", "first")
        );
        assert_eq!(rows[0].tags, ["work", "home"]);
        assert_eq!(store.read(note.id.clone()).unwrap().tags, ["work", "home"]);
        let work = store.notes_tagged("work".into()).unwrap();
        assert_eq!(work.len(), 1);
        assert!(store.notes_tagged("play".into()).unwrap().is_empty());
        assert_eq!(store.tags_in_use().unwrap(), ["home", "work"]);
        let found = store.search("TITLE".into()).unwrap();
        assert_eq!(
            (found[0].id.as_str(), found[0].title_matched),
            (note.id.as_str(), true)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
