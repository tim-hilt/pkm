# One visible folder per Context, one Sync engine with a merge hook each

Supersedes the folder layout of ADR-0003 (Drawings in Documents) and ADR-0011 (Drawings in the cloud's `pkm` folder). Builds on ADR-0016 and ADR-0017.

Each Context keeps its Items as plain files in a folder of its own: `Drawings/` in Documents on Apple, in the Origin Private File System on the web, and `pkm/Drawings/` with the Cloud Provider (a folder per Context in `pkm`; later `Books/`). One Sync engine Syncs a set of Context folders, each matched with its cloud folder and with its own `Merge` hook (which files are Items, how two versions become one; Drawings: the per-Element merge of ADR-0011). Sync state is kept per Context folder (`Sync/<Context>/` on Apple, `Cloud/state/<Context>/` on the web). The rules of ADR-0011 (when to read, delete, upload, rename a clash) stay in the engine and are the same for every Context. Progress and status cover all folders together; a folder's files never reach another folder.

## Considered Options

- **Context folders directly in Documents and `pkm`, with Drawings still at the root**: no migration, but Drawings would be the one Context without a folder of its own, and `Books/` would sit among Drawing files.
- **One engine run per Context, with its own status**: simpler to write, but the user sees one Cloud Backing, so statuses and progress would have to be merged by every client.

## Consequences

- Drawings saved before this change sit in the old locations (Documents, `pkm/`) and have to be moved into `Drawings/` by hand.
- A Cloud Provider storage is scoped to one Context folder; Reconnect hands the loop one storage per folder.
- The web app Syncs its one Context folder with the same step machine; several are a loop over steps, as natively.
- Progress counts the transfers of the folders that have run, so its total grows while later folders come up.
- A Delete wins over edits made elsewhere, in every Context folder alike (ADR-0011); a file never Synced is uploaded, never deleted.
- A rename on another Device arrives as a delete and an add. A step deletes local files last, so the new file is there before the old one goes, and an open Drawing follows it by its Item ID (ADR-0019) instead of closing.
