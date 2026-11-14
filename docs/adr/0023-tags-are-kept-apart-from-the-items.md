# Tags are kept apart from the Items, in one file merged per Tag

Builds on ADR-0020 (a folder and merge hook per Context folder).

A Tag lives until it is Deleted, not only while an Item carries it, and has a Tag Color shared by every Device. So the Tags are kept in one file, `Tags/tags.json`, beside the Context folders and Synced by the same engine. Its merge hook merges per Tag: a Tag Color changed on both sides takes the newer, a Delete wins over a Tag Color changed elsewhere. Listing and suggesting Tags reads this file and no Item.

## Considered Options

- **Tags only on the Items** (as before): no extra state, but listing them reads every Item, Closed ones included, and nowhere keeps a Tag Color.
- **One file per Tag**: merges by the engine's file rules alone, but Tag names hold `/` and need escaping, and launch reads many files.

## Consequences

- A Tag found on an Item but not in the file (a Device on an older version, a hand edit, a Tag added on one Device while another Deleted it) is added back, grey. A Delete therefore never strips a Tag someone put on an Item concurrently.
- Tags carried before this change are added to the file by a one-off migration script.
