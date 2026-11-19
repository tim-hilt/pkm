//! The Notes surface of the wasm exports, mirroring `crates/bindings/src/notes.rs`.
//!
//! The store works on files in memory as the Tasks' does: the client hands in
//! the files it has saved when opening, and after each change saves the files
//! `take_changes` names.

use pkm_notes::{MemoryFiles, Notes};
use std::path::Path;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// A Note's Title and Body as typed, see `pkm_notes::Note`.
#[wasm_bindgen(getter_with_clone)]
pub struct Note {
    pub id: String,
    pub title: String,
    pub body: String,
}

impl From<pkm_notes::Note> for Note {
    fn from(note: pkm_notes::Note) -> Self {
        Self {
            id: note.id,
            title: note.title,
            body: note.body,
        }
    }
}

/// A Note as one row of the list, see `pkm_notes::Row`.
#[wasm_bindgen(getter_with_clone)]
pub struct ListedNote {
    pub id: String,
    pub title: String,
    /// The Body's line to show when the Title is empty; format it like a Title.
    pub preview: String,
    /// UTC ISO 8601.
    pub updated: String,
}

impl From<pkm_notes::Row> for ListedNote {
    fn from(row: pkm_notes::Row) -> Self {
        Self {
            id: row.id,
            title: row.title,
            preview: row.preview,
            updated: row.updated,
        }
    }
}

/// The Notes, see `pkm_notes::Notes`.
#[wasm_bindgen]
pub struct NoteStore {
    files: Arc<MemoryFiles>,
    notes: Notes,
}

fn error(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

#[wasm_bindgen]
impl NoteStore {
    /// The folder the Notes are kept in, in the app's storage.
    pub fn folder() -> String {
        Notes::FOLDER.to_owned()
    }

    /// Opens the Notes from the saved files: `paths[i]` held `texts[i]`, in
    /// storage paths (`Notes/<id>.md`).
    #[wasm_bindgen(constructor)]
    pub fn new(paths: Vec<String>, texts: Vec<String>) -> NoteStore {
        let files = Arc::new(MemoryFiles::default());
        for (path, text) in paths.into_iter().zip(texts) {
            files.load(Path::new(&path), text);
        }
        let notes = Notes::open_in(files.clone(), Notes::FOLDER);
        Self { files, notes }
    }

    /// The paths written or removed since the last call; `fileBytes` holds
    /// what to save, none for a removed one.
    #[wasm_bindgen(js_name = takeChanges)]
    pub fn take_changes(&self) -> Vec<String> {
        (self.files.take_changes().into_iter())
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// What the file at `path` holds now; none if there is no such file.
    #[wasm_bindgen(js_name = fileBytes)]
    pub fn file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        use pkm_notes::Files;
        self.files.read_bytes(Path::new(path)).ok()
    }

    /// Saves a new, empty Note.
    pub fn create(&self) -> Result<Note, JsError> {
        self.notes.create().map(Into::into).map_err(error)
    }

    pub fn read(&self, id: &str) -> Result<Note, JsError> {
        self.notes.read(id).map(Into::into).map_err(error)
    }

    /// Sets the Title and Body as typed and bumps `updated`.
    pub fn edit(&self, id: &str, title: &str, body: &str) -> Result<Note, JsError> {
        (self.notes.edit(id, title, body))
            .map(Into::into)
            .map_err(error)
    }

    pub fn delete(&self, id: &str) -> Result<(), JsError> {
        self.notes.delete(id).map_err(error)
    }

    /// Deletes the Note if its Title and Body are empty; for leaving it.
    #[wasm_bindgen(js_name = discardIfEmpty)]
    pub fn discard_if_empty(&self, id: &str) -> Result<(), JsError> {
        self.notes.discard_if_empty(id).map_err(error)
    }

    /// Every Note, newest `updated` first.
    pub fn notes(&self) -> Result<Vec<ListedNote>, JsError> {
        let rows = self.notes.list().map_err(error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
