# Closing a Task Deletes it, and no History is kept

Supersedes ADR-0022, and the Closed-Task parts of ADR-0021 (the `done`/`rejected` states and the Device cache).

Done and Rejected both Delete the Task's file, and those of its Open Subtasks, once the Revert toast is gone. The History log is removed. Closed Task files and the History piled up on every Device and in the cloud for an analysis that never came (YAGNI). Done and Rejected stay two actions: the intent differs, and either can change on its own later.

## Consequences

- Revert still works: the Close is only written when the toast goes away.
- A Close against an edit on another Device is a Delete against an edit, so the Close wins (ADR-0020).
- A Blocked by pointing at a Closed Task points at a missing one, and is dropped as ADR-0021 already does.
- Every Task file is Open, and there are few, so the Device cache and the Device ID (which only named History files) go. The migration script (ADR-0025) Deletes existing Closed files and `Tasks/.history/`.
