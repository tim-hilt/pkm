# Notes are flat files organised by Tags, not Folders

Supersedes ADR-0030's Folders and ADR-0031. Builds on ADR-0021, ADR-0023 and ADR-0029.

Folders made the user file each Note in exactly one place. Tags let one Note belong to several topics and organise Notes the same way as Tasks and Bookmarks. Each Note is `Notes/<id>.md`, with front matter `id`, `title`, `updated` and `tags`, and its Images sit beside it. Sync merges `tags` as a set, as for Tasks. The list is narrowed to exactly one Tag, as for Bookmarks.

## Considered Options

- **Keep Folders and add Tags**: Notes would have two ways of being organised, and Sync would keep the nested paths only Folders need.
- **Inline `#tags` in the Body** (Obsidian): the Body would have to be parsed, and the rules would have to say what wins when the Body and the front matter disagree.

## Consequences

- Storages, Cloud Providers and Sync are flat again: no directory walk, and no matching of Notes by Item ID across paths.
- Notes still in subdirectories of `Notes/` are ignored, and there is no migration.
