# The Tasks History is a log by Item ID beside the Task files

History is kept for later analysis, apart from the Task files: each Device appends lines (`<time> <event> <id>`) to its own file per month, `Tasks/.history/<device>-<yyyy-mm>.log`; events are created, made-active, moved-to-backlog, done and rejected. Tasks are never Deleted in the app (Reject takes its place), but a Task file removed outside the app takes nothing of the History with it, and the lines hold no Title, Tag or other content.

## Considered Options

- **History in the Task's file**: no extra files, but it grows the file meant for people, and every event would go through the three-way merge.

## Consequences

- Each Device writes only its own files, so Sync never merges them; reading the History unites all Devices' files.
- A Close is saved, to the Task's file and the History, only when its Revert toast goes away (or the app goes to the background). Revert so leaves no trace, and History has no `reverted` event.
