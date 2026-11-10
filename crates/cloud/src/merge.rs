//! What the Sync engine asks of a Context (ADR-0020): which files are its
//! Items, and how two versions of one Item become one. The engine decides
//! when to merge; the Context decides how.

use pkm_books::sync as books;
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

    /// Whether Sync may transfer bytes that `is_item` rejects. True by
    /// default: a Drawing saved by another tool still reaches the other
    /// Devices. False for a Context whose files Sync must understand, e.g.
    /// Books' state files: an unparseable one is left where it is.
    fn transfers_unparseable(&self) -> bool {
        true
    }

    /// Whether two versions that can't merge, changed on both sides, are both
    /// kept: the local one under the next free name. False by default: the
    /// local one stays and the conflict is left alone.
    fn keeps_both_versions(&self) -> bool {
        false
    }

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

/// Book Files: the PDFs and EPUBs of the Books Context. They are opaque: Sync
/// transfers them unchanged and never merges two versions. Same-named Files
/// of different content are both kept, the local one under the next free name.
pub struct Books;

impl Books {
    /// The Context's folder on the Device and in the cloud (`pkm/Books`).
    pub const FOLDER: &str = "Books";
}

impl Merge for Books {
    fn extension(&self) -> &str {
        ".pdf"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        if local == remote {
            Some(local.to_vec())
        } else if base == Some(remote) {
            // Only this Device changed the File: the cloud takes it.
            Some(local.to_vec())
        } else if base == Some(local) {
            // Only the cloud's File changed: this Device takes it.
            Some(remote.to_vec())
        } else {
            // Both changed differently: never merged.
            None
        }
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        bytes.starts_with(b"%PDF-") || bytes.starts_with(b"PK\x03\x04")
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        Some(a == b)
    }

    fn keeps_both_versions(&self) -> bool {
        true
    }

    fn accepts(&self, file: &str) -> bool {
        let lower = file.to_ascii_lowercase();
        !file.starts_with('.') && (lower.ends_with(".pdf") || lower.ends_with(".epub"))
    }

    fn name_of(&self, file: &str) -> String {
        file.rsplit_once('.')
            .map_or(file, |(stem, _)| stem)
            .to_owned()
    }

    fn numbered(&self, file: &str, n: u32) -> String {
        match file.rsplit_once('.') {
            Some((stem, extension)) => format!("{stem} {n}.{extension}"),
            None => format!("{file} {n}"),
        }
    }
}

/// The Books' state files: `.state/<id>.json` and `.state/preferences.json` in
/// the Books folder, merged by the Books merge hook (`pkm_books::sync`).
pub struct BookStates;

impl Merge for BookStates {
    fn extension(&self) -> &str {
        ".json"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        books::merge(base, local, remote)
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        books::is_item(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        books::same_item(a, b)
    }

    fn transfers_unparseable(&self) -> bool {
        false
    }
}
