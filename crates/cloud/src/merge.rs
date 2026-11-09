//! What the Sync engine asks of a Context (ADR-0020): which files are its
//! Items, and how two versions of one Item become one. The engine decides
//! when to merge; the Context decides how.

use pkm_drawing::{are_same_drawing, is_drawing, merge_drawings};

/// The merge hook of one Context folder.
pub trait Merge: Send + Sync {
    /// The extension, with its dot, of the files that are Items of this
    /// Context. Empty: every file. Other files are left alone.
    fn extension(&self) -> &str;

    /// What `local` and `remote` merge to, given the Item's bytes at the last
    /// Sync (none if there was none). None if either isn't an Item.
    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>>;

    /// Whether `bytes` are an Item `merge` can handle.
    fn is_item(&self, bytes: &[u8]) -> bool;

    /// Whether two same-named files with no Sync history between them are one
    /// Item. None if either isn't an Item.
    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool>;

    /// Whether a file of this name is an Item of the Context.
    fn accepts(&self, file: &str) -> bool {
        let extension = self.extension();
        file.len() > extension.len() && file.ends_with(extension) && !file.starts_with('.')
    }

    /// The file name without the extension.
    fn name_of(&self, file: &str) -> String {
        file.strip_suffix(self.extension())
            .unwrap_or(file)
            .to_owned()
    }

    /// The file name for the `n`-th variant of `file`: "Sketch 2.excalidraw".
    fn numbered(&self, file: &str, n: u32) -> String {
        format!("{} {n}{}", self.name_of(file), self.extension())
    }
}

/// Drawings: `.excalidraw` files, merged Element by Element (ADR-0011).
pub struct Drawings;

impl Drawings {
    /// The Context's folder on the Device and in the cloud (`pkm/Drawings`).
    pub const FOLDER: &str = "Drawings";
}

impl Merge for Drawings {
    fn extension(&self) -> &str {
        ".excalidraw"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        merge_drawings(base, local, remote).ok()
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        is_drawing(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        are_same_drawing(a, b).ok()
    }
}
