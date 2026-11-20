//! What the Sync engine asks of a Context (ADR-0020): which files are its
//! Items, and how two versions of one Item become one. The engine decides
//! when to merge; the Context decides how.

use pkm_base::tags::Tags;
use pkm_books::sync as books;
use pkm_drawing::{are_same_drawing, is_drawing, merge_drawings};
use pkm_notes::sync as notes;
use pkm_tasks::sync::{self as tasks, filters};

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

    /// Whether the Context keeps its Items in directories (ADR-0031). Then
    /// Items are paired by their path from the Context folder, `/` separated,
    /// and directories, empty ones too, are Synced. False by default: Items
    /// are the files directly in the folder.
    fn has_directories(&self) -> bool {
        false
    }

    /// The Item's identity, the same wherever it lives, none if `bytes` have
    /// none (ADR-0031). With one, Sync pairs Items by it and follows an Item
    /// Moved to another path; without, by path alone. Default: none.
    fn identity(&self, _bytes: &[u8]) -> Option<String> {
        None
    }

    /// Whether `local` wins over `remote`, two versions of one Item Moved to
    /// different paths: the Move of the newer one stands. Only asked for Items
    /// with an `identity`.
    fn local_is_newer(&self, _local: &[u8], _remote: &[u8]) -> bool {
        true
    }

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

/// The merge of a Context whose files are opaque: the side that changed one
/// since `base` wins, and two different changes never merge.
fn transfer(base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
    if local == remote || base == Some(remote) {
        // Same, or only this Device changed the file: the cloud takes it.
        Some(local.to_vec())
    } else if base == Some(local) {
        // Only the cloud's file changed: this Device takes it.
        Some(remote.to_vec())
    } else {
        None
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
        transfer(base, local, remote)
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

/// Tasks: `<id>.md` files, merged three-way by field (ADR-0021), and the
/// Images beside them, transferred unchanged (ADR-0029). A file that
/// isn't a Task is left where it is, and two versions that can't merge are
/// not both kept: one Task has one file.
pub struct Tasks;

impl Tasks {
    /// The Context's folder on the Device and in the cloud (`pkm/Tasks`).
    pub const FOLDER: &str = pkm_tasks::Tasks::FOLDER;
}

impl Merge for Tasks {
    fn extension(&self) -> &str {
        ".md"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        tasks::merge(base, local, remote)
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        tasks::is_item(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        tasks::same_item(a, b)
    }

    fn accepts(&self, file: &str) -> bool {
        tasks::accepts(file)
    }

    fn transfers_unparseable(&self) -> bool {
        false
    }
}

/// Notes: `<id>.md` files in Folder directories, merged three-way, the Body by
/// line (ADR-0032), and the Images beside them, transferred unchanged
/// (ADR-0029, ADR-0031). A file that isn't a Note is left where it is, and two versions that can't
/// merge are not both kept: one Note has one file.
pub struct Notes;

impl Notes {
    /// The Context's folder on the Device and in the cloud (`pkm/Notes`).
    pub const FOLDER: &str = pkm_notes::Notes::FOLDER;
}

impl Merge for Notes {
    fn extension(&self) -> &str {
        ".md"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        notes::merge(base, local, remote)
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        notes::is_item(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        notes::same_item(a, b)
    }

    fn accepts(&self, file: &str) -> bool {
        notes::accepts(file)
    }

    fn identity(&self, bytes: &[u8]) -> Option<String> {
        notes::identity(bytes)
    }

    fn local_is_newer(&self, local: &[u8], remote: &[u8]) -> bool {
        notes::local_is_newer(local, remote)
    }

    fn has_directories(&self) -> bool {
        true
    }

    fn transfers_unparseable(&self) -> bool {
        false
    }
}

/// The Tags file: `Tags/tags.json` beside the Context folders, merged per Tag
/// (ADR-0023). One file, so any two Tags files are one Item; a file that
/// isn't one is left where it is.
pub struct TagsFile;

impl TagsFile {
    /// The folder on the Device and in the cloud (`pkm/Tags`).
    pub const FOLDER: &str = "Tags";
}

impl Merge for TagsFile {
    fn extension(&self) -> &str {
        ".json"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        Tags::merge(base, local, remote)
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        Tags::is_file(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        (Tags::is_file(a) && Tags::is_file(b)).then_some(true)
    }

    fn transfers_unparseable(&self) -> bool {
        false
    }
}

/// Filters: `Filters/<id>.md` beside the Tasks folder, merged three-way by
/// field with `places` per Task ID (ADR-0024). A file that isn't a Filter is
/// left where it is.
pub struct Filters;

impl Filters {
    /// The folder on the Device and in the cloud (`pkm/Filters`).
    pub const FOLDER: &str = pkm_tasks::Tasks::FILTERS_FOLDER;
}

impl Merge for Filters {
    fn extension(&self) -> &str {
        ".md"
    }

    fn merge(&self, base: Option<&[u8]>, local: &[u8], remote: &[u8]) -> Option<Vec<u8>> {
        filters::merge(base, local, remote)
    }

    fn is_item(&self, bytes: &[u8]) -> bool {
        filters::is_item(bytes)
    }

    fn same_item(&self, a: &[u8], b: &[u8]) -> Option<bool> {
        filters::same_item(a, b)
    }

    fn transfers_unparseable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_notes_keep_their_items_in_directories() {
        assert!(Notes.has_directories());
        let flat: [&dyn Merge; 6] = [&Drawings, &Books, &BookStates, &Tasks, &TagsFile, &Filters];
        assert!(flat.iter().all(|merge| !merge.has_directories()));
    }

    #[test]
    fn only_notes_give_their_items_an_identity() {
        let note = b"---\nid: aaaaaaaaaaaa\ntitle: x\nupdated: 2024-01-01T00:00:00Z\n---\n";
        assert_eq!(Notes.identity(note).as_deref(), Some("aaaaaaaaaaaa"));
        assert_eq!(Tasks.identity(note), None);
        assert_eq!(Drawings.identity(b"{}"), None);
    }
}
