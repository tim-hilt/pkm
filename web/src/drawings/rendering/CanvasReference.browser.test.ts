import { beforeAll, describe, it } from "vitest";
import { expectMatchesReference } from "../testing/BrowserReferences";
import { type RenderHarness, defaultViewport, makeRenderHarness } from "../testing/OffscreenRender";
import sampleUrl from "../../../generated/sample.excalidraw?url";

/**
 * Seam 2: what the Canvas draws, compared with stored reference images, a
 * port of the Swift `CanvasReferenceTests` (see
 * docs/web-renderer-reference-images.md for how to re-record them). Text and
 * images have test files of their own.
 */

let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

describe("Canvas reference images", () => {
  it("draws a rectangle's bounds", async () => {
    const image = await harness.render(harness.scenes.rectangle());
    await expectMatchesReference(image, "rectangle-bounds");
  });

  it("draws a rectangle zoomed and panned", async () => {
    const image = await harness.render(harness.scenes.rectangle(), {
      originX: 40,
      originY: 90,
      zoom: 2,
    });
    await expectMatchesReference(image, "rectangle-bounds-zoomed");
  });

  it("draws a rectangle on a retina screen", async () => {
    const image = await harness.render(harness.scenes.rectangle(), defaultViewport, {
      pixelsPerPoint: 2,
      size: 800,
    });
    await expectMatchesReference(image, "rectangle-bounds-retina");
  });

  /** The bundled sample: rotated box, deleted and unknown Elements not shown. */
  it("draws the sample Drawing", async () => {
    const bytes = new Uint8Array(await (await fetch(sampleUrl)).arrayBuffer());
    const image = await harness.render(harness.scenes.geometryOfBytes(bytes), defaultViewport, {
      size: 700,
    });
    await expectMatchesReference(image, "sample-drawing");
  });

  /** Dashed, dotted, translucent, thick, rounded and every sloppiness. */
  it("draws hand-drawn styles", async () => {
    const image = await harness.render(harness.scenes.handDrawnStyles(), defaultViewport, {
      size: 420,
    });
    await expectMatchesReference(image, "hand-drawn-styles");
  });

  /** Hachure, cross-hatch, zigzag and solid, plus architect and thick variants. */
  it("draws every fill style", async () => {
    const image = await harness.render(harness.scenes.fillStyles(), defaultViewport, { size: 420 });
    await expectMatchesReference(image, "fill-styles");
  });

  /** Ellipse, diamond, rounded and rotated rectangle, translucent solid. */
  it("draws filled shapes", async () => {
    const image = await harness.render(harness.scenes.filledShapes(), defaultViewport, {
      size: 420,
    });
    await expectMatchesReference(image, "filled-shapes");
  });

  /** Concave and self-intersecting outlines by the non-zero and even-odd rules. */
  it("draws the fill rules", async () => {
    const image = await harness.render(harness.scenes.fillRules());
    await expectMatchesReference(image, "fill-rules");
  });

  /** Straight, multi-point, curved, closed, rotated and dashed lines and arrows, and arrowheads. */
  it("draws lines and arrows", async () => {
    const image = await harness.render(harness.scenes.linesAndArrows(), defaultViewport, {
      size: 520,
    });
    await expectMatchesReference(image, "lines-and-arrows");
  });

  /** Strokes with simulated and real pressure, thick, translucent, rotated, and filled loops. */
  it("draws freedraw strokes", async () => {
    const image = await harness.render(harness.scenes.freedraw());
    await expectMatchesReference(image, "freedraw");
  });
});
