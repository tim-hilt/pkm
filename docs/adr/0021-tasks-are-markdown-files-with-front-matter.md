# Tasks are Markdown files with front matter, merged three-way

Builds on ADR-0019 (Item IDs) and ADR-0020 (a folder and merge hook per Context).

Each Task is `Tasks/<id>.md`: a front matter of fixed keys (`id`, `title`, `priority`, `state` (`active`, `backlog`, `done` or `rejected`), `parent`, `position`, `tags`, `blocked-by`, `updated`) and the Description as the body. The front matter is a YAML subset of scalars and block lists (`- item`) only, read and written by `pkm-base` so that Notes can reuse it. The file is named by Item ID, so changing the Title never renames it. The merge hook merges three-way against the file at the last Sync: a field changed on one side takes that side, tags and blocked-by merge as sets, and a field changed on both sides takes the side with the newer `updated`.

## Considered Options

- **JSON**: already in `pkm-base` and simplest to merge, but clunky to read or edit by hand, and Notes will be Markdown anyway.
- **One file for all Tasks** (todo.txt): every change would conflict with every other.
- **A change time per field**: merges without a base, but clutters a file meant for people.

## Consequences

- Sync can leave structures no Device allowed. They are repaired on read, never refused: an orphaned Subtask becomes top-level, a Parent cycle is cut at the smallest Item ID, a Blocked by pointing at a missing Task is ignored and dropped at the next save.
- An Open Subtask stores its top-level Task's `active` or `backlog` too, so each file reads alone; moving a top-level Task rewrites its Open Subtasks, and on read the top-level Task's wins.
- Each Device caches every file's id, `state` and Title until its modification date changes, never Synced, so launch reads only changed files and keeps only Open Tasks in memory.
- Manual order is `position`, a fractional key per Task in its whole Active or Backlog list, Subtasks included, so a move rewrites one file; equal keys tie-break by Item ID. A move never changes the `parent`.
- Files from before `position` have an `order` among siblings instead. On read, an Open Task without `position` gets one, derived from the tree order those files showed (top-level Tasks by `order`, each followed by its Subtasks, depth first) and placed after the Tasks that have one; `order` is dropped and `updated` kept. A file with `position` is never derived again.
