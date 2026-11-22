# Images are files beside their Task, Deleted with it

Builds on ADR-0021 and ADR-0026.

A pasted Image is kept as `Tasks/<random-id>.<ext>` and referenced from the Description as `![](<random-id>.<ext>)`, which Obsidian also reads. Sync transfers Images unmerged, like Book Files (ADR-0020). PNG, JPEG, GIF and WebP are kept byte for byte. Anything else (such as TIFF, HEIC or SVG) is stored as PNG. Images are never downscaled.

## Considered Options

- **One app-wide `Attachments/` folder**: text could move between Contexts, but deleting an Image would need reference counts across Contexts.
- **Base64 inline in the `.md`**: no extra files, but it bloats every merge and makes the raw line unreadable.

## Consequences

- An Image is Deleted when its Task is Closed (once Revert is gone) or Deleted, or when the sheet closes after its last reference was removed, as long as no other Task references it. The same holds for a New Task sheet that is cancelled.
- There is no general sweep: an Image can Sync to a Device before the Task that references it, and a sweep would delete it.
- Two Devices editing one Description keep the newer one whole (ADR-0021), so a reference can be lost while its Image file stays.
