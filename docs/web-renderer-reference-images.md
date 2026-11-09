# Web renderer reference image tests

The web app's counterpart of [renderer-reference-images.md](renderer-reference-images.md):
the TS renderer draws geometry offscreen with WebGPU in Chromium (Vitest
browser mode with Playwright), and the result is compared with PNG files in
`web/references`. They need a GPU, so they stay out of `make check`:

```sh
cd web && bunx playwright-core install chromium   # once
make web-test-browser
```

## Writing a test

Browser tests are `web/src/**/*.browser.test.ts`; the Node tests of
`make web-test` leave them alone.

```ts
const harness = await makeRenderHarness();           // GPU, core and scenes
const image = await harness.render(harness.scenes.rectangle(), viewport, { size: 400 });
await expectMatchesReference(image, "my-scene");     // compares with references/my-scene.png
```

`render` uses the production `CanvasRenderer` including 4x MSAA via
`renderOffscreen`; `pixelsPerPoint` simulates the screen scale (use 2 with a
larger `size`). The scenes in `src/testing/TestScenes.ts` are ports of the
Swift `TestScenes`, with the same names and seeds; so the references can be
compared by eye with `apple/pkmTests/References`.

## Tolerance

As in Swift: a pixel counts as different when any channel differs by more
than 8/255, and a render matches while at most 0.02 % of its pixels differ
(`standardTolerance` in `src/testing/ImageComparison.ts`). Pass a stricter
`ImageTolerance` to `expectMatchesReference` for a specific test.

## When a test fails

The message names three files in `web/reference-failures` (not committed):
`<name>-actual.png`, `<name>-expected.png` (the committed reference) and
`<name>-diff.png`, which is the reference with every differing pixel in red.
They are deleted again once the check passes.

## Recording references

Recording is always explicit:

```sh
make web-record-references
```

This runs the browser tests with `RECORD_REFERENCES` set. References that are
missing or no longer match are rewritten; matching ones are left untouched, so
the run produces no churn. Afterwards:

1. Look at `git status` and open the changed PNGs; they must show what you intended.
2. Run `make web-test-browser` to confirm everything passes without recording.
3. Commit the PNGs together with the code change that caused them.

A normal run never writes a reference; a missing one is a failure. The image
references (`images*`) are in `ImageRendering.browser.test.ts`.
