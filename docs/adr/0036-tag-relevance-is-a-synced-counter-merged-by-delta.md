# Tag Relevance is a synced counter in the Tags file, merged by delta

Builds on ADR-0023.

Every list of Tags is ordered by Relevance, so it is the same on every Device. Each Tag in `Tags/tags.json` keeps a `relevance` count, starting at 0 and raised by one each time the Tag is put on an Item. The merge hook adds each side's change since the last Sync to the base: `base + (local − base) + (remote − base)`. A reset to 0 is then just another change and merges cleanly. For a Tag missing from the last Sync, the higher count wins. Changing only Relevance does not bump `updated`, which still decides the Tag Color.

## Considered Options

- **Per Device, not synced**: simplest, but each Device would order the same Tags differently.
- **A count per Device, summed (G-Counter)**: needs no base, but the file grows with every Device, and a reset needs a counter of its own.
- **Frecency** (recent uses weigh more): forgets old uses on its own, but is more complex and cannot be shown as one plain number. A manual reset does the forgetting instead.

## Consequences

- Existing Tags start at 0; there is no migration from the Items carrying them.
- A Tag added back because it is found on an Item (ADR-0023) starts at 0.
