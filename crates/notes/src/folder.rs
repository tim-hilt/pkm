//! Folders: names and the tree (ADR-0030). A Folder is a path of names
//! relative to `Notes/`, joined by `/`; the top level is the empty path.

use std::io;

/// A Folder and the Folders in it, by name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Folder {
    pub name: String,
    /// Names from the top level, joined by `/`.
    pub path: String,
    pub folders: Vec<Folder>,
}

/// What a Folder holds, all the way down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Counts {
    pub notes: u32,
    pub folders: u32,
}

/// Why a Folder couldn't be made, renamed or Moved.
#[derive(Debug)]
pub enum FolderError {
    /// The name is empty (only whitespace counts as empty).
    Empty,
    /// The name has a `/`.
    HasSlash,
    /// The name starts with `.`, which would hide it.
    Hidden,
    /// A Folder of that name, whatever the case, is there already.
    Taken,
    /// A Folder can't be Moved into itself or a Folder in it.
    Inside,
    /// No such Folder, or the storage failed.
    Io(io::Error),
}

impl From<io::Error> for FolderError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl std::fmt::Display for FolderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => f.write_str("The name is empty."),
            Self::HasSlash => f.write_str("The name can't contain a /."),
            Self::Hidden => f.write_str("The name can't start with a dot."),
            Self::Taken => f.write_str("A Folder of that name is already there."),
            Self::Inside => f.write_str("A Folder can't be moved into itself."),
            Self::Io(e) => e.fmt(f),
        }
    }
}

/// The name as kept (without surrounding whitespace), or why it is refused.
/// Clashes with siblings are the store's to check.
pub(crate) fn check_name(name: &str) -> Result<&str, FolderError> {
    let name = name.trim();
    if name.is_empty() {
        Err(FolderError::Empty)
    } else if name.contains('/') {
        Err(FolderError::HasSlash)
    } else if name.starts_with('.') {
        Err(FolderError::Hidden)
    } else {
        Ok(name)
    }
}

/// Whether `name` is one a client may name in a path: no `.`, `..`, nothing
/// hidden, no `/`.
pub(crate) fn is_visible_name(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains('/')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_refused_with_a_reason() {
        assert!(matches!(check_name(""), Err(FolderError::Empty)));
        assert!(matches!(check_name("  "), Err(FolderError::Empty)));
        assert!(matches!(check_name("a/b"), Err(FolderError::HasSlash)));
        assert!(matches!(check_name(".git"), Err(FolderError::Hidden)));
        assert_eq!(check_name(" Work ").unwrap(), "Work");
    }
}
