//! The Notes surface of the UniFFI library (ADR-0007): the Notes store. The
//! logic lives in `pkm-notes`.

use pkm_notes::{Counts, Folder, FolderError, Found, Note, Notes, Row};

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

/// A Folder and the Folders in it, see `pkm_notes::Folder`.
#[uniffi::remote(Record)]
pub struct Folder {
    pub name: String,
    pub path: String,
    pub folders: Vec<Folder>,
}

/// What a Folder holds, all the way down.
#[uniffi::remote(Record)]
pub struct Counts {
    pub notes: u32,
    pub folders: u32,
}

/// Why a Folder couldn't be made, renamed or Moved, see `pkm_notes::FolderError`.
#[derive(Debug, uniffi::Error)]
pub enum FolderNameError {
    Empty,
    HasSlash,
    Hidden,
    Taken,
    Inside,
    Failed { message: String },
}

impl std::fmt::Display for FolderNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { message } => f.write_str(message),
            other => write!(f, "{other:?}"),
        }
    }
}

impl From<FolderError> for FolderNameError {
    fn from(e: FolderError) -> Self {
        match e {
            FolderError::Empty => Self::Empty,
            FolderError::HasSlash => Self::HasSlash,
            FolderError::Hidden => Self::Hidden,
            FolderError::Taken => Self::Taken,
            FolderError::Inside => Self::Inside,
            FolderError::Io(e) => Self::Failed {
                message: e.to_string(),
            },
        }
    }
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

/// A Note that Search found, see `pkm_notes::Found`.
#[derive(uniffi::Record)]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    /// The path of its Folder; empty for the top level.
    pub folder: String,
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
            folder: found.folder,
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

    /// Saves a new, empty Note in the Folder at `folder` (empty for the top level).
    pub fn create(&self, folder: String) -> Result<Note, NotesError> {
        Ok(self.0.create(&folder)?)
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

    /// The Notes in the Folder at `folder` (empty for the top level), newest `updated` first.
    pub fn notes(&self, folder: String) -> Result<Vec<ListedNote>, NotesError> {
        Ok(self.0.list(&folder)?.into_iter().map(Into::into).collect())
    }

    /// The Notes in any Folder with `query` in the Title or Body, Title matches first, each newest first.
    pub fn search(&self, query: String) -> Result<Vec<SearchResult>, NotesError> {
        Ok(self.0.search(&query)?.into_iter().map(Into::into).collect())
    }

    /// The Folders at the top level, with the Folders in them, by name.
    pub fn folders(&self) -> Result<Vec<Folder>, NotesError> {
        Ok(self.0.folders()?)
    }

    /// Makes a Folder in `parent` and returns its path.
    pub fn create_folder(&self, parent: String, name: String) -> Result<String, FolderNameError> {
        Ok(self.0.create_folder(&parent, &name)?)
    }

    /// Renames the Folder at `path` and returns its new path.
    pub fn rename_folder(&self, path: String, name: String) -> Result<String, FolderNameError> {
        Ok(self.0.rename_folder(&path, &name)?)
    }

    /// Moves the Note into the Folder at `folder` (empty for the top level).
    pub fn move_note(&self, id: String, folder: String) -> Result<(), NotesError> {
        Ok(self.0.move_note(&id, &folder)?)
    }

    /// Moves the Folder at `path` into the Folder `into` and returns its new path.
    pub fn move_folder(&self, path: String, into: String) -> Result<String, FolderNameError> {
        Ok(self.0.move_folder(&path, &into)?)
    }

    /// Deletes the Folder at `path` with everything in it.
    pub fn delete_folder(&self, path: String) -> Result<(), NotesError> {
        Ok(self.0.delete_folder(&path)?)
    }

    /// The Notes and Folders in the Folder at `path`, all the way down.
    pub fn count(&self, path: String) -> Result<Counts, NotesError> {
        Ok(self.0.count(&path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_written_through_the_bindings_is_listed() {
        let dir = std::env::temp_dir().join(format!("pkm-bindings-notes-{}", std::process::id()));
        let store = NoteStore::open(dir.join("Notes").to_string_lossy().into_owned()).unwrap();
        let note = store.create(String::new()).unwrap();
        store
            .edit(note.id.clone(), "Title".into(), "- first\n".into())
            .unwrap();
        let rows = store.notes(String::new()).unwrap();
        assert_eq!(
            (rows[0].title.as_str(), rows[0].preview.as_str()),
            ("Title", "first")
        );
        let found = store.search("TITLE".into()).unwrap();
        assert_eq!(
            (found[0].id.as_str(), found[0].title_matched),
            (note.id.as_str(), true)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
