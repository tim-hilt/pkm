# The cloud holds plain `.excalidraw` files, merged Element by Element

Cloud Backing keeps each Drawing as a plain `.excalidraw` file in the Cloud Provider's `pkm/Drawings` folder (ADR-0020), readable by excalidraw.com and other tools (ADR-0003). The Device's Documents folder stays the source of truth; the cloud is a copy Synced with it, for every Cloud Provider alike, iCloud Drive included. Conflicts are merged per Element the way Excalidraw's collaboration does: the higher `version` wins, `versionNonce` breaks ties, erased Elements stay as `isDeleted` so that erasing merges too, and the `files` map is united by id. Each Device remembers the state of each Drawing at its last Sync. The merge is in the core, pure and order-independent.

## Considered Options

- **The earlier reader app's design (Device-owned Automerge chunks, immutable content-hashed files)**: no write can be lost, but the cloud would hold unreadable chunks, the core would gain a CRDT dependency, and the model would duplicate the one the file format already has.

## Consequences

- Google Drive and iCloud Drive have no conditional writes, so a write can overwrite another Device's write. The merge heals it: the overwritten Device sees its newer Elements missing on its next poll and writes them again.
- Whole files are uploaded on every change, images included.
- Two Drawings of the same name are one Drawing, and merged, if they share an Element id (or are both empty); otherwise both are kept and the local one takes the next free name ("Sketch 2"). This needs no Sync history, so Connecting again after a Disconnect, which forgets that history, splits nothing; a Drawing deleted while Disconnected comes back, though.
- A Drawing is its file name: a rename reaches other Devices as a delete and an add, so a rename crossing an edit leaves only the renamed Drawing.
- A Delete wins over edits made elsewhere at the same time, in every Context: a Device that finds a file gone from the cloud that it had at its last Sync deletes its copy, edited or not. A never-Synced file is uploaded, never deleted. An open Drawing deleted by Sync is closed and not written back.
- Remote changes merged into the open Drawing are not undo steps.
- `appState` (background colour, grid) has no versions; it is merged per key against the last Synced state, the cloud winning when both changed a key.
