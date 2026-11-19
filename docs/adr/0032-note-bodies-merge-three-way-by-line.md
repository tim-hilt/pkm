# A Note's Body merges three-way by line

Amends ADR-0021's "the newer side wins" for Notes.

When two Devices changed one Note, its Body merges three-way against the version at the last Sync, line by line: changes to different lines are all kept, and where both sides changed the same lines the side with the newer `updated` wins those lines. `title` and the Folder take the newer side whole. The merge lives in `pkm-base`, without dependencies, so Task Descriptions can use it later.

## Considered Options

- **Newer Body wins whole**, as for a Task's Description: silently loses a paragraph written on the other Device, which matters for long texts.
- **Keep both as a conflict copy**: loses nothing, but leaves clutter to clean up by hand.
- **Conflict markers in the Body**: loses nothing, but breaks the Markdown until fixed.
