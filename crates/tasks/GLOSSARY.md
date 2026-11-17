# Tasks

The Context for things to do, seen through Filters. App-wide terms (Context, Item, Item ID, Delete, Tag, Device, Cloud Backing, Sync) are in the [App glossary](../../GLOSSARY.md).

## Language

**Task**:
One thing to do, with a Title, a Description, a Priority and Tags.
_Avoid_: Todo, item (when meaning a Task specifically)

**Title**:
The one line naming a Task, required; what lists show first.
_Avoid_: Name, summary

**Description**:
Plain text the user writes about a Task.
_Avoid_: Note (that is a Context of its own), body, details

**Priority**:
How urgent a Task is: P1 (highest) to P4, the default.
_Avoid_: Importance, level

### Quick Add

**Quick Add**:
Typing a Task's Title with Shortcuts in it, picked out as they are typed.
_Avoid_: Smart recognition, magic input, natural language input

**Shortcut**:
A part of the typed Title that sets a property instead, like `p1` for a Priority or `@work` for a Tag.
_Avoid_: Token, magic input, marker

**Ignore**:
Turning a Shortcut back into plain text of the Title.
_Avoid_: Escape, dismiss, unmark

### Closing

**Open**:
A Task not yet Closed.
_Avoid_: Pending, todo, Active

**Close**:
Ending a Task for good as **Done** (finished) or **Rejected** (given up on, or never meant), with its Open Subtasks. A Closed Task is Deleted, leaving no trace.
_Avoid_: Complete, check off, resolve, cancel, drop, delete

**Revert**:
Taking back a Close in the moment after it, as if it never happened.
_Avoid_: Undo, Reopen, restore

### Order

**Manual order**:
The order the user drags a Filter's Tasks into; Tasks not yet dragged follow at the end.
_Avoid_: Custom order, position

**Subtask order**:
The one order of a Parent's Subtasks, the same in its sheet and in every Filter: the order they are worked on in.
_Avoid_: Sibling order, child order

**Sort**:
A Filter's automatic order by a **Sort key** (for now only Priority) in a **Direction** (Ascending or Descending), Subtasks by their own; Tasks with the same Sort key keep their Manual order, and Subtasks of one Parent always keep their Subtask order. A Filter is either **Sorted** or in Manual order; dragging a Task to a different Sort key puts it in Manual order.
_Avoid_: View, sort order, arrangement

### Filters

**Filter**:
A Title and a Query, showing the Open Tasks the Query matches, in its own Manual order or Sort. Not an Item. Every list of Tasks is a Filter.
_Avoid_: View, smart list, saved search, perspective, Active, Backlog, inbox

**Query**:
The expression a Filter matches Tasks by, on their own Tags, e.g. `@work & !@in-review`.
_Avoid_: Filter (that is the saved whole), rule, search

**Wildcard**:
A `*` in a Query's Tag, standing for any characters or none, so that it matches every Tag that fits: `@*backlog`, `@*family*`, `@*` for any Tag.
_Avoid_: Glob, pattern, regex

**Favorite**:
A Filter shown in the Tasks bar, to be reached in one step.
_Avoid_: Pin, star, shortcut (that is Quick Add's)

**Delete a Filter**:
Ending a Filter on every Device; no Task changes.
_Avoid_: Remove, close

### Structure

**Subtask**:
A Task under another Task, its **Parent**. A Task has at most one Parent, at any depth.
_Avoid_: Child, nested task, checklist item

**Blocked by**:
A Task is Blocked by another while that one is Open; it can still be Closed.
_Avoid_: Depends on, prerequisite, Link (that is a reference between Items)

**Blocker**:
A Task another is Blocked by.
_Avoid_: Dependency, prerequisite
