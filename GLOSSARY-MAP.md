# Glossary Map

## Contexts

- [App](./GLOSSARY.md): the Contexts, their items, Links between them, Settings and Cloud Backing
- [Tasks](./crates/tasks/GLOSSARY.md): Tasks with Priorities and Tags, seen through Filters, with Subtasks, possibly Blocked by others
- [Notes](./crates/notes/GLOSSARY.md): Notes with a Title and a Body, in Folders, found by Search
- [Drawings](./crates/drawing/GLOSSARY.md): Excalidraw-compatible Drawings, edited on the Canvas
- [Bookmarks](./crates/bookmarks/GLOSSARY.md): saved web pages, some with an Article to read and mark up
- [Books](./crates/books/GLOSSARY.md): PDFs and EPUBs to read and mark up, with Highlights and Comments

## Relationships

- **App → Notes**: Delete and Cloud Backing are the app's; Cloud Backing Syncs Folders as directories, merging each Note's Body by line.

- **App → Drawings**: Cloud Backing Syncs the Drawings with the Drawings Context's own merge.
- **App → Bookmarks**: Delete, Tags, Reading Progress, Markup and Cloud Backing are the app's.
- **App → Books**: Reading Progress and Markup are the app's, shared with Bookmarks; Delete, Appearance and Cloud Backing are the app's; Page Style "Match Appearance" follows the app's Appearance. Cloud Backing Syncs Book Files unchanged and merges their state files.
