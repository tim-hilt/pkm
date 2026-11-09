# Renderer reference image tests

Seam 2 of the test strategy (`.scratch/v1/spec.md`): the Swift renderer draws
geometry offscreen with Metal, and the result is compared with PNG files stored
in `apple/pkmTests/References`. `make test` runs them on macOS,
`make test-ios` on a simulator.

## Writing a test

```swift
let image = try OffscreenRender.render(geometry, viewport: Viewport(), size: 400).image
expectMatchesReference(image, named: "my-scene")   // compares with References/my-scene.png
```

`OffscreenRender` uses the production `CanvasRenderer` including 4x MSAA.
`pixelsPerPoint` simulates the screen scale (use 2 or 3 with a larger `size`).

## Tolerance

A pixel counts as different when any channel differs by more than 8/255, and
a render matches while at most 0.02 % of its pixels differ (`ImageTolerance`).
On one machine renders are identical, and the iOS simulator uses the Mac's
GPU, so this only has to absorb anti-aliasing noise between GPUs; a moved,
missing or recoloured element, or a 10 % change of opacity, is beyond it. Pass a stricter `ImageTolerance` to `expectMatchesReference` for a
specific test if needed.

## When a test fails

The message names three files in `pkmReferenceFailures` inside the
test process's temporary directory (for the sandboxed macOS host that is
`~/Library/Containers/com.timhilt.pkm/Data/tmp`): `<name>-actual.png`, the committed reference and
`<name>-diff.png`, which is the reference with every differing pixel in red.
They are deleted again once the check passes.

## Recording references

Recording is always explicit:

```sh
make record-references
```

This runs the tests with `RECORD_REFERENCES` set and the app sandbox switched
off (the sandbox forbids writing into the source tree). References that are missing or
no longer match are rewritten; matching ones are left untouched, so the run
produces no churn. Afterwards:

1. Look at `git diff --stat` and open the changed PNGs; they must show what you intended.
2. Run `make test` to confirm everything passes without recording.
3. Commit the PNGs together with the code change that caused them.

A normal test run never writes a reference; a missing one is a failure.
References are recorded on macOS (Apple silicon GPU); the tolerance covers
the small differences on other Apple GPUs.
