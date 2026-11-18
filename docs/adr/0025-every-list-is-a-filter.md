# Every list of Tasks is a Filter, ordered and Sorted in its own file

Builds on ADR-0024; supersedes the Active/Backlog `state` and the `position` of ADR-0021.

Active and the Backlog are no longer lists of their own. Backlog is the Tag `backlog`, and every list is a Filter: All Tasks is `*`, Active is `!@backlog`, Backlog is `@backlog`, and each can be edited or Deleted like any other. A Filter's file keeps its Manual order (`places`) and its Sort (`sort: manual | priority ascending | priority descending`). Task files keep neither, only a `created` time that orders the Tasks no Filter has placed yet, and a Subtask's `subtask-position` among its siblings: its Subtask order, the same in its Parent's sheet and every Filter. One way of listing and ordering serves every list, and the Sort Syncs with the Filter.

## Considered Options

- **Keep Active and Backlog, with their Sort in a settings file** (`Tasks/.lists.md`): a smaller change, but two kinds of list with two order mechanisms, and Filters still blind to the Backlog.
- **Keep `position` in the Task files for All Tasks**: no order to migrate, but again two order mechanisms.

## Consequences

- Creating a Task writes no Filter file: unplaced Tasks follow the placed ones by `created`. A Sort writes nothing at all to the Task files.
- A Filter decides which rows a Parent's Subtasks take; the Subtask order decides which sibling takes which. So a Sorted Filter can show a P3 Subtask above its P1 sibling, and a Filter drag past a sibling rewrites that Subtask's file.
- `sort` is one scalar, so the Sort key and Direction merge together, the newer `updated` winning. An unknown value is kept and read as Manual, so a later Sort key doesn't break older Devices.
- A one-off script on the cloud folder (`scripts/migrations/migrate-lists.py`, run while every Device is synced and idle) tags Backlog Tasks, drops `state`, `position` and `order`, and creates the three Filters as Favorites. Existing Filters get `& !@backlog` added to their Query, so they keep showing what they showed. The app reads the new format only; a new Device starts with no Filters.
