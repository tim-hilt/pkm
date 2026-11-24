# Dropping a Task onto another makes it a Subtask

Amends ADR-0025: a drag in a Filter can now change a Task's `parent`.

A drag in a Filter either moves a Task between rows (its Manual order, ADR-0025) or drops it onto another row of its Group, which makes it that Task's Subtask: `parent` is set and `subtask-position` goes last among its new siblings, writing only its file. It keeps its Tags and its own Subtasks. In Manual order the Filter also moves it just below the last row of its new Parent and that one's Subtasks, so that the rows match the Subtask order; a Sorted Filter keeps its Sort. A drop onto the Task itself or one of its Subtasks is refused.

## Considered Options

- **A Parent picker in the sheet**: removed before (`.scratch/subtasks-in-sheet`) as clutter; a drop is where the user already orders Tasks.
- **Take on the new Parent's Tags**, as a new Subtask does: the Task would jump Groups or leave the Filter under the pointer.

## Consequences

- Making a Subtask top-level again has no gesture yet.
- The Subtasks section of a sheet still only reorders.
