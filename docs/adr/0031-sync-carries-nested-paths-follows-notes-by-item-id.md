# Sync carries nested paths and follows Notes by Item ID across them

Supersedes the "one flat folder" storage of ADR-0020 (which stays otherwise). Needed by ADR-0030.

A storage lists, reads, writes and deletes files by a path relative to its Context folder, `/` separated, and keeps empty directories; every Cloud Provider walks subdirectories itself (WebDAV at Depth 1 per directory). Contexts without directories see no change. For Notes, Sync pairs files by the Item ID in their front matter, not by path: a Note Moved on one Device and edited on another ends up Moved and edited, and two different Moves of one Note take the newer. A Folder renamed differently on two Devices thus ends up with the newer name, and a directory emptied by such Moves is removed. Images follow their Note by their unique file name and transfer unmerged.

## Considered Options

- **Path-only matching**, as for Drawings (ADR-0020): a Move arrives as a delete and an add, so a Move against an edit loses the edit, and a Folder rename against any edit inside it duplicates Notes.
- **Flat cloud folder, with the path kept in the file**: no Cloud Provider change, but the cloud would no longer mirror the Device, and Folders would still need a record of their own.

## Consequences

- A Delete a Folder wins over edits inside it elsewhere (ADR-0020); a Note created in or Moved into it elsewhere lands at the top level instead.
- Sync state records each Note's path at the last Sync beside its Item ID, to tell a Move from a delete and an add.
