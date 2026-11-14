# Tasks

The Context for things to do, planned as Active or in the Backlog. App-wide terms (Context, Item, Item ID, Delete, Tag, Device, Cloud Backing, Sync) are in the [App glossary](../../GLOSSARY.md).

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

### Planning

**Open**:
A Task not yet Closed: either Active or in the Backlog.
_Avoid_: Pending, todo

**Active**:
An Open top-level Task the user plans to do now, with its Subtasks.
_Avoid_: Current, in progress, today

**Backlog**:
The Open top-level Tasks the user plans to do later, with their Subtasks.
_Avoid_: Inbox, someday, later

**Close**:
Ending a Task for good as **Done** (finished) or **Rejected** (given up on, or never meant). Tasks are Rejected, not Deleted.
_Avoid_: Complete, check off, resolve, cancel, drop, delete

**Revert**:
Taking back a Close in the moment after it, as if it never happened.
_Avoid_: Undo, Reopen, restore

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

**History**:
When Tasks were created, made Active, moved to the Backlog, Done and Rejected, by Item ID only, for later analysis.
_Avoid_: Log, audit trail, events
