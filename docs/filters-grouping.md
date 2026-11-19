# Filters and Grouping

A Filter is a saved list of Tasks (ADR-0024, 0025, 0027): a `title`, a Query, a Sort and optional Group by. Stored as `Filters/<id>.md`.

## Query

```text
or   := and ('|' and)*
and  := not ('&' not)*
not  := '!' not | atom
atom := '*' | '@' tag | '(' or ')'
```

| Term | Meaning |
| --- | --- |
| `@tag` | Task carries exactly that Tag |
| `@work*` | Wildcard: `*` is any characters or none, `/` included (`work`, `work/acme`) |
| `*` | every Task, Tagged or not |
| `!x` | not `x` |
| `x & y` | both |
| `x \| y` | either |
| `( … )` | grouping |

- Precedence: `!` over `&` over `|`.
- Whitespace between terms is ignored; uppercase is lowercased as typed.
- Tags: `a-z`, `0-9`, `-`, `/`; `/` never leads, trails or doubles.
- Parentheses nest at most 64 deep.
- Syntax errors report a character position.
- A Query naming an unknown or Deleted Tag is kept as written.
- A Task created in a Filter gets the plain `@tag` terms (no Wildcards) if the Query is a single `&` chain; otherwise none.

Built-in examples: All Tasks `*`, Active `!@backlog`, Backlog `@backlog`. Another: `@work & !@waiting | @urgent`.

## Sort

`sort:` is `manual` (default), `priority ascending` or `priority descending`. Unknown values read as Manual.

Manual order is kept in `places`; Tasks without a place follow by `created`.

## Group by

`group-by` is an ordered list of Tags, typed with or without `@`, separated by whitespace (`@work @private`).

- Only existing Tags; no Wildcards. Repeats are dropped.
- A Task with several of the Tags appears in each Group, with its own Manual order per Group.
- Tasks with none of them sit in no Group.
- Dragging across Groups isn't possible; change the Task's Tags instead.
- Removing a Tag from Group by drops its places. Deleting a Tag rewrites Filters that Group by it.

## File format

```yaml
id: …
title: …
query: "@work & !@backlog"
group-by: work private
sort: priority ascending
favorite: true
position: 0
updated: …
places:
  - <task-id> <position>          # no Group
  - <tag> <task-id> <position>    # in a Group
```
