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

/// A Folder as one row of the outline: the tree flattened, parents before
/// their Folders, each level by name.
#[wasm_bindgen(getter_with_clone)]
pub struct FolderRow {
    pub name: String,
    /// Names from the top level, joined by `/`.
    pub path: String,
    /// How many Folders it is in: 0 at the top level.
    pub depth: u32,
}

fn flatten(level: Vec<pkm_notes::Folder>, depth: u32, rows: &mut Vec<FolderRow>) {
    for folder in level {
        rows.push(FolderRow {
            name: folder.name,
            path: folder.path,
            depth,
        });
        flatten(folder.folders, depth + 1, rows);
    }
}

/// What a Folder holds, all the way down.
#[wasm_bindgen]
pub struct Counts {
    pub notes: u32,
    pub folders: u32,
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
    /// storage paths (`Notes/<folder>/<id>.md`); `folders` are the storage
    /// paths of the Folders saved, empty ones too.
    #[wasm_bindgen(constructor)]
    pub fn new(paths: Vec<String>, texts: Vec<String>, folders: Vec<String>) -> NoteStore {
        use pkm_notes::Files;
        let files = Arc::new(MemoryFiles::default());
        for (path, text) in paths.into_iter().zip(texts) {
            files.load(Path::new(&path), text);
        }
        for folder in folders {
            // Memory makes any folder.
            drop(files.make_folder(Path::new(&folder)));
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

    /// Saves a new, empty Note in the Folder at `folder` (empty for the top level).
    pub fn create(&self, folder: &str) -> Result<Note, JsError> {
        self.notes.create(folder).map(Into::into).map_err(error)
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

    /// The Notes in the Folder at `folder` (empty for the top level), newest `updated` first.
    pub fn notes(&self, folder: &str) -> Result<Vec<ListedNote>, JsError> {
        let rows = self.notes.list(folder).map_err(error)?;
        Ok(rows.into_iter().map(Into::into).collect())
    }

    /// Every Folder, as the outline shows them.
    pub fn folders(&self) -> Result<Vec<FolderRow>, JsError> {
        let mut rows = Vec::new();
        flatten(self.notes.folders().map_err(error)?, 0, &mut rows);
        Ok(rows)
    }

    /// Makes a Folder in `parent` and returns its path; the error says why
    /// the name is refused.
    #[wasm_bindgen(js_name = createFolder)]
    pub fn create_folder(&self, parent: &str, name: &str) -> Result<String, JsError> {
        self.notes.create_folder(parent, name).map_err(error)
    }

    /// Renames the Folder at `path` and returns its new path.
    #[wasm_bindgen(js_name = renameFolder)]
    pub fn rename_folder(&self, path: &str, name: &str) -> Result<String, JsError> {
        self.notes.rename_folder(path, name).map_err(error)
    }

    /// Moves the Note into the Folder at `folder` (empty for the top level).
    #[wasm_bindgen(js_name = moveNote)]
    pub fn move_note(&self, id: &str, folder: &str) -> Result<(), JsError> {
        self.notes.move_note(id, folder).map_err(error)
    }

    /// Moves the Folder at `path` into the Folder `into` and returns its new
    /// path; the error says why a Move is refused.
    #[wasm_bindgen(js_name = moveFolder)]
    pub fn move_folder(&self, path: &str, into: &str) -> Result<String, JsError> {
        self.notes.move_folder(path, into).map_err(error)
    }

    /// Deletes the Folder at `path` with everything in it.
    #[wasm_bindgen(js_name = deleteFolder)]
    pub fn delete_folder(&self, path: &str) -> Result<(), JsError> {
        self.notes.delete_folder(path).map_err(error)
    }

    /// The Notes and Folders in the Folder at `path`, all the way down.
    pub fn count(&self, path: &str) -> Result<Counts, JsError> {
        let counts = self.notes.count(path).map_err(error)?;
        Ok(Counts {
            notes: counts.notes,
            folders: counts.folders,
        })
    }
}
