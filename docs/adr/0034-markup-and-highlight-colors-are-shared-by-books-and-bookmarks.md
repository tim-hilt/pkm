# Markup and the Highlight Colors are shared by Books and Bookmarks; the colors leave the Books folder

Builds on ADR-0017, ADR-0020 and ADR-0023.

Bookmarks mark up Articles as Books mark up their text, in the same Highlight Colors. So the Markup model (Highlights, Comments, colors), its JSON and its merge live in `pkm-base` (`markup`), generic over each Context's anchors: Books keeps its PDF and EPUB anchors, Bookmarks bring offsets. The Highlight Colors preferences (built-in colors, Custom Colors, Default Highlight Color) live there too (`highlight_colors`), and are stored apart from the Books: `Colors/colors.json`, beside the Context folders like `Tags/tags.json`, Synced by the same engine with its own merge hook (`ColorsFile`, per color, a removal wins).

Moving the file changes Books' Sync: the colors are no longer an Item of the Books state folder, so a Custom Color added in Books reaches Bookmarks, and every Device, through the one file.

## Considered Options

- **A shared crate for Markup**: separate from `pkm-base`, but `pkm-base` already holds what Contexts share, has no dependencies and keeps `pkm-drawing`'s as they are.
- **Leaving the colors in the Books folder**: no move, but Bookmarks would have to depend on Books or keep a second palette.

## Consequences

- A one-off migration in `Library` carries `.state/preferences.json` into the new file (merged with what is there) and deletes it, at opening and at every listing, so a copy that Sync brings from a Device on an older version is taken in too. Devices on an older version don't see the new file until they update.
- `.state/preferences.json` is no Item to Books' merge hook any more: it is left where it is.
