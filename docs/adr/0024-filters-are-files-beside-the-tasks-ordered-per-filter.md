# Filters are files beside the Tasks, each with its own order of Tasks

> Amended by ADR-0025: Filters match every Open Task, keep a `sort`, and unplaced Tasks follow by `created`, not Active's order.

Builds on ADR-0020 (a folder and merge hook per Context folder) and ADR-0021 (Tasks as front-matter files).

Each Filter is `Filters/<id>.md`, named by a random ID like an Item's (ADR-0019) though it is no Item. Its front matter has `id`, `title`, `query`, `favorite`, `position` (its place in the Filters list), `updated` and `places`. `places` is a block list of `<task-id> <position>` entries: the Filter's own order of the Tasks it shows. `Filters/` is one more Sync folder. Its merge hook works like the Tasks': three-way per field, with the newer `updated` winning when both sides changed a field. The exception is `places`, which merges per Task ID: a place changed on only one side keeps that side's value, and a place changed on both sides takes the newer side's.

## Considered Options

- **One `Filters/filters.json`, merged per Filter** (like ADR-0023's Tags): one read at launch, but every Filter edit and every drag would rewrite a file shared by all Filters.
- **Places stored in each Task file**, as `<filter-id> <position>` entries: no merge rule beyond sets, but files meant for people would fill up with Filter details, and a drag inside a Filter would rewrite a Task.

## Consequences

- A drag inside a Filter rewrites only that Filter's file; Task files never mention Filters.
- Tasks without a place in `places` are shown after the placed ones, in Active's order. The first drag gives every Task shown a place, in one write.
- Whenever a Filter's file is written, places of Tasks the Filter no longer shows are dropped. A Task that matches again later starts without a place.
- A Query naming an unknown or Deleted Tag is kept as written; Delete a Tag never rewrites Filters.
