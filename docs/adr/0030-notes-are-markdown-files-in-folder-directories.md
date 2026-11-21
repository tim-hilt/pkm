# Notes are Markdown files named by Item ID, in directories that are their Folders

> Superseded in part: Folders by ADR-0035; Notes are `Notes/<id>.md`.

Builds on ADR-0019, ADR-0021 and ADR-0029.

Each Note is `Notes/<Folder path>/<id>.md`: front matter (`id`, `title`, `updated`) read and written by `pkm-base` as for Tasks, and the Body as the file's body. A Folder is a real directory, so the tree reads the same in Finder, Files and Obsidian and can be edited there; renaming a Folder renames its directory. The file is named by Item ID, so changing the Title never renames it. Images sit beside their Note as `<random-id>.<ext>` (ADR-0029) and move with it unless another Note still shows them. Directories starting with `.` and files that are neither `.md` nor Images are ignored.

## Considered Options

- **Folders as records in one `Notes/folders.json`, each Note naming its Folder's Item ID**: a rename rewrites one file and Sync stays flat, but the tree can't be seen or edited outside the app.
- **A Folder path in each Note's front matter**: flat files, but moving a Folder rewrites every Note under it, and empty Folders need a record elsewhere.
- **File named after the Title** (Obsidian): readable, but every Title edit is a rename, and OneDrive forbids characters a Title may hold. Left for later.

## Consequences

- Sync has to carry directories (ADR-0031).
- Notes files made by hand (no `id`, no `title`) are not handled yet.
