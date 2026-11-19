//! The Notes surface of the UniFFI library (ADR-0007): the Notes store. The
//! logic lives in `pkm-notes`.

use pkm_notes::{Note, Notes, Row};

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
}

impl From<Row> for ListedNote {
    fn from(row: Row) -> Self {
        Self {
            id: row.id,
            title: row.title,
            preview: row.preview,
            updated: row.updated,
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

    /// Saves a new, empty Note.
    pub fn create(&self) -> Result<Note, NotesError> {
        Ok(self.0.create()?)
    }

    pub fn read(&self, id: String) -> Result<Note, NotesError> {
        Ok(self.0.read(&id)?)
    }

    /// Sets the Title and Body as typed and bumps `updated`.
    pub fn edit(&self, id: String, title: String, body: String) -> Result<Note, NotesError> {
        Ok(self.0.edit(&id, &title, &body)?)
    }

    pub fn delete(&self, id: String) -> Result<(), NotesError> {
        Ok(self.0.delete(&id)?)
    }

    /// Deletes the Note if its Title and Body are empty; for leaving it.
    pub fn discard_if_empty(&self, id: String) -> Result<(), NotesError> {
        Ok(self.0.discard_if_empty(&id)?)
    }

    /// Every Note, newest `updated` first.
    pub fn notes(&self) -> Result<Vec<ListedNote>, NotesError> {
        Ok(self.0.list()?.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_written_through_the_bindings_is_listed() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-notes-{}", std::process::id()));
        let store = NoteStore::open(dir.join("Notes").to_string_lossy().into_owned()).unwrap();
        let note = store.create().unwrap();
        store
            .edit(note.id.clone(), "Title".into(), "- first\n".into())
            .unwrap();
        let rows = store.notes().unwrap();
        assert_eq!(
            (rows[0].title.as_str(), rows[0].preview.as_str()),
            ("Title", "first")
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
