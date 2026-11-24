# The Unit is a pkm-specific key in `appState`

A Drawing's Unit is stored as `appState.pkmUnit`, the number of Scene pixels per unit. It is only written once the user sets it; without it, the Unit is one Scene pixel. It lives in the Drawing because Dimensions must read the same on every Device. `appState` is where the core already keeps unknown members unchanged and where Cloud Backing already merges key by key, so it needs no new file and no new merge.

## Considered Options

- **A sidecar file beside the Drawing**: survives excalidraw.com, but Drawings are plain `.excalidraw` files (ADR-0003), and a second file would need syncing, renaming and deleting along with the Drawing.
- **A per-Device setting**: the same Element would read differently on different Devices.

## Consequences

- excalidraw.com drops `pkmUnit` when it saves a Drawing. The Drawing then falls back to one Scene pixel per unit.
