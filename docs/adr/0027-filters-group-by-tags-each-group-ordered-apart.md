# A Filter Groups by Tags, each Group in its own Manual order

Builds on ADR-0024 and ADR-0025.

A Filter can Group by an ordered list of Tags. A Task carrying several of them shows in each of their Groups, so each Group keeps its own Manual order: a drag in one Group never moves the Task in another. The Filter file keeps `group-by` (Tags separated by spaces) and, in `places`, `<tag> <task-id> <position>` for a place in a Group beside `<task-id> <position>` for one among the Tasks of no Group. Tag names hold no spaces, so the two forms are told apart by their field count, and Filters without Group by keep their files as they are.

## Considered Options

- **One Manual order per Filter, shared by its Groups**: `places` unchanged, but a drag in one Group could move the Task in another.
- **A Task only in its first Group**: one place per Task, but a Task serving two Groups would hide from one.

## Consequences

- `places` merges per Group and Task ID; `group-by` is one scalar, the newer `updated` winning, like `sort`.
- A new Group starts from the order the Filter showed; taking a Tag out of Group by drops its places.
- Unlike a Query (ADR-0024), Group by names only existing Tags: Delete a Tag rewrites every Filter that Groups by it. A Group-by Tag missing from the Tags file (a Delete racing an edit) is added back, grey, as in ADR-0023.
- Dragging across Groups is not possible; a Task changes Groups by changing its Tags.
