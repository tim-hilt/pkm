# Visual parity review with excalidraw.com

The spec's manual check that Drawings look the same as on excalidraw.com
(ADR-0004). A fixed set of fixture Drawings is exported as PNG from
excalidraw.com and from the app, and the pairs are compared by eye.

## Fixtures

`scripts/visual-parity/fixtures.mjs` writes `scripts/visual-parity/fixtures/*.excalidraw`
(deterministic; ids and seeds are fixed):

| Fixture    | Covers |
| ---------- | ------ |
| `shapes`   | rectangle, diamond, ellipse × hachure / cross-hatch / solid fill, unfilled with round edges, stroke and background colours |
| `strokes`  | stroke width 1 / 2 / 4 × solid / dashed / dotted, sloppiness 0 / 1 / 2, opacity 100 / 60 / 20, rotation |
| `lines`    | every arrowhead (start and end), curved and sharp arrows, multi-point line, closed filled line, dashed / dotted arrows |
| `freedraw` | simulated and recorded pressure, stroke widths, opacity, a closed loop |
| `text`     | font sizes 16–36, left / center / right alignment, multi-line, colour, non-ASCII, text bound to a container |
| `images`   | embedded PNG: plain, stretched, 50 % opacity, rotated, mirrored |

## Producing the screenshots

```sh
make visual-parity       # both sides; needs network and Google Chrome
make visual-parity-app   # the app side only
```

- **excalidraw.com** (`scripts/visual-parity/excalidraw.mjs`, Playwright with
  the system Chrome): opens each fixture like a user (Open), then Export image →
  PNG at 2× with background. Writes `docs/visual-parity/<name>-excalidraw.png`.
  `EXCALIDRAW_URL` points it at another deployment, `HEADED=1` shows the browser,
  fixture names as arguments limit the run.
- **App** (`VisualParityTests`, macOS): `PNGExport` of each fixture (2×,
  background), written to `docs/visual-parity/<name>-app.png` when
  `RECORD_VISUAL_PARITY` is set. A normal `make test` only checks that every
  fixture opens and exports.

## Findings (excalidraw.com, November 2026)

| Fixture    | Size app / excalidraw | Result |
| ---------- | --------------------- | ------ |
| `shapes`   | 1400 × 1220 / 1400 × 1220 | Identical strokes and fills, seed for seed. |
| `strokes`  | 1304 × 1210 / 1304 × 1209 | Identical; 1 px rounding of the export height. |
| `lines`    | 1160 × 1000 / 1163 × 1001 | Same shapes and arrowheads. Export frame 3 px narrower (see issue 52). Dot / circle arrowheads look slightly smoother in the app. |
| `freedraw` | 700 × 640 / 700 × 700 | **Significant:** the app's export frame ignores points above the first point and clips the top of the waves. Stroke outlines themselves match. → issue 52. |
| `text`     | 1162 × 780 / 1162 × 780 | Glyphs, sizes, alignment and line height match. The bound label is centred by the app, whereas excalidraw.com draws it at the (inaccurate) stored `x`/`width` of the fixture until it is edited; not a defect. |
| `images`   | 1290 × 810 / 1289 × 809 | Identical, including opacity, rotation, mirroring and smoothing; 1 px rounding of the export size. |

Minor differences (1–3 px export size rounding, arrowhead smoothness, bound
text placement for un-normalised files) are recorded here only. Significant
ones are filed as issues: **52** (export frame from points).

Since issue 52 (export frame from points) and review R12 (pixel size rounded
like Excalidraw's `exportToCanvas`, not up), the app exports every fixture at
excalidraw.com's size; `VisualParityTests` asserts it. The `*-app.png` files
predate both until `make visual-parity-app` is run again.
