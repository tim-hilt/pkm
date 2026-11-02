# Golden fixture generator

Generates the golden fixtures in `crates/core/tests/fixtures/golden` (ADR-0004):
for a fixed set of Elements and seeds, the exact drawing ops and outline points
that Excalidraw produces. `cargo test` only reads the committed output, so Node
is needed solely to (re)generate it.

## Run

```sh
make golden-fixtures
# or by hand:
cd scripts/golden && npm ci && node generate.mjs [path/to/excalidraw]
```

The Excalidraw checkout defaults to `../../excalidraw/excalidraw` relative to
this repository; override it with the argument or `EXCALIDRAW_DIR`. It needs no
`npm install` of its own: its element code is bundled with esbuild and the few
third-party packages it imports come from this directory's `node_modules`.

The output is deterministic. Regenerating without changing anything upstream
must leave `git status` clean.

## Pinned upstream versions

| Upstream           | Version / commit                           | Enforced by |
| ------------------ | ------------------------------------------ | ----------- |
| Excalidraw         | `8bcdca4526867433f99fdfcb55a063668db0c9dc` | `EXCALIDRAW_COMMIT` in `upstream.mjs`; the script refuses any other `HEAD` |
| rough.js           | `4.6.4`                                    | `package.json`; the script refuses a version differing from the checkout's |
| perfect-freehand   | `1.2.0`                                    | same |

The other packages in `package.json` (`points-on-curve`, `tinycolor2`, `nanoid`,
...) are pinned to the checkout's versions too, so the bundled code behaves the
same. To move to a newer Excalidraw, update the commit, the versions, and
regenerate; the diff of the fixtures is then the upstream behaviour change.
Every golden file records the versions it was generated from.

## Files

- `generate.mjs`: writes the fixtures.
- `cases.mjs`: which Elements are generated (the coverage matrix). Ids name the
  Element, and seeds are derived from the id, so adding a case never changes
  another case's output.
- `upstream.mjs`: loads the checkout, builds Elements with Excalidraw's own
  constructors and asks Excalidraw's `ShapeCache` / freedraw code what to draw.

Fixture format: `crates/core/tests/fixtures/golden/README.md`.
