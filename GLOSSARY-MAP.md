# Glossary Map

## Contexts

- [App](./GLOSSARY.md): the Contexts, their items, Links between them, Settings and Cloud Backing
- [Tasks](./crates/tasks/GLOSSARY.md): Tasks with Priorities and Tags, seen through Filters, with Subtasks, possibly Blocked by others
- [Drawings](./crates/drawing/GLOSSARY.md): Excalidraw-compatible Drawings, edited on the Canvas
- [Books](./crates/books/GLOSSARY.md): PDFs and EPUBs to read and mark up, with Highlights and Comments

## Relationships

- **App → Drawings**: Cloud Backing Syncs the Drawings with the Drawings Context's own merge.
- **App → Books**: Delete, Appearance and Cloud Backing are the app's; Page Style "Match Appearance" follows the app's Appearance. Cloud Backing Syncs Book Files unchanged and merges their state files.
