# Item IDs are random and live in the Item

Builds on ADR-0003 (plain Excalidraw files) and ADR-0009 (no dependencies).

Every Item gets an Item ID for life: 12 lowercase base32 characters (`a-z`, `2-7`, 60 bits) from `pkm-base`'s random generator, which also parses them. A Drawing keeps its Item ID in a top-level `"id"` property of its `.excalidraw` file, so it survives renames, edits and Sync with nothing recorded beside the file. Excalidraw ignores properties it does not know, so the file still opens on excalidraw.com.

Both clients list Drawings through their store, which keeps the rules in one place: a Drawing without an Item ID (dropped in, or re-saved by excalidraw.com, which drops the property) gets one written, and of several Drawings with the same one (a copy made in Finder) all but the oldest get a new one (`pkm_base::item_id::claim`). A file that is no Drawing cannot hold one and is found by its name. Clients open and track Drawings by Item ID; the name stays the file name.

## Considered Options

- **A sidecar index** (name → Item ID): breaks when a file is renamed or copied outside the app, which Files and Finder allow.
- **UUIDs**: 36 characters in every link, for no gain over 60 random bits.
- **A hash of the first content**: changes meaning as the Drawing is edited, and two empty Drawings would collide.

## Consequences

- A file saved by excalidraw.com gets a new Item ID, so links to it break; accepted, as the app cannot stop that.
- Listing rewrites the file of a Drawing that gets an Item ID, which makes it the newest Drawing and Syncs it.
- The clients remember the Item ID of each file until its modification date changes, so that listing reads only changed files.
