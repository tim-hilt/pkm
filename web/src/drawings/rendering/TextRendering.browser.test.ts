import { beforeAll, describe, expect, it } from "vitest";
import { expectMatchesReference } from "../testing/BrowserReferences";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";
import { drawing, textElement } from "../testing/TestScenes";
import { type RenderedImage, redAt } from "./RenderedImage";

/**
 * Renders text offscreen and probes pixels, a port of the Swift
 * `TextRenderingTests`: paper is red 255 and ink 30. Also the reference
 * images of text (`text`, `text-zoomed`).
 */

let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

/** Pixels in the given rectangle (device pixels) that are clearly ink. */
function ink(frame: RenderedImage, x: [number, number], y: [number, number]): number {
  let count = 0;
  for (let row = y[0]; row < y[1]; row++) {
    for (let column = x[0]; column < x[1]; column++) {
      if (redAt(frame, column, row) < 128) count++;
    }
  }
  return count;
}

const render = (elements: string[], viewport = { originX: 0, originY: 0, zoom: 1 }) =>
  harness.render(harness.scenes.geometryOf(drawing(...elements)), viewport);

const hello = textElement("t", 50, 50, [100, 50], "Hello", `,"fontSize":40,"lineHeight":1.25`);

describe("text", () => {
  it("is ink inside its box and nothing outside", async () => {
    const frame = await render([hello]);

    expect(ink(frame, [50, 160], [50, 110]), "the letters").toBeGreaterThan(150);
    expect(ink(frame, [0, 400], [120, 400]), "below the text").toBe(0);
    expect(ink(frame, [170, 400], [0, 400]), "to the right of the text").toBe(0);
  });

  it("right-aligned ends at the right edge of its box", async () => {
    const frame = await render([
      textElement("t", 50, 50, [300, 50], "Hi", `,"fontSize":40,"textAlign":"right"`),
    ]);

    expect(ink(frame, [50, 250], [0, 400]), "the left of the box is empty").toBe(0);
    expect(ink(frame, [250, 355], [50, 110]), "the text is at the right").toBeGreaterThan(50);
  });

  it("keeps its colour and opacity", async () => {
    const frame = await render([
      textElement("t", 50, 50, [100, 50], "Hello", `,"fontSize":40,"strokeColor":"#e03131"`),
    ]);

    // #e03131 has red 224; the darkest pixels of the text are that and never the default ink.
    let darkest = 255;
    for (let x = 50; x < 160; x++)
      for (let y = 50; y < 110; y++) darkest = Math.min(darkest, redAt(frame, x, y));
    expect(darkest).toBe(224);
  });

  it("is drawn in z-order with the shapes around it", async () => {
    const cover = `{"id":"c","type":"rectangle","x":40,"y":40,"width":140,"height":80,"roughness":0,"backgroundColor":"#a5d8ff","fillStyle":"solid","strokeColor":"transparent"}`;

    const under = await render([hello, cover]);
    expect(ink(under, [50, 160], [50, 110]), "a later shape hides the text").toBe(0);

    const over = await render([cover, hello]);
    expect(ink(over, [50, 160], [50, 110]), "text over a shape is visible").toBeGreaterThan(150);
    expect(redAt(over, 45, 45), "and the shape is still there").toBe(165);
  });

  it("is sharp at any zoom", async () => {
    // The H of "Hello" at 12x: its strokes are many pixels thick, so a crisp
    // edge leaves few pixels in between ink and paper.
    const frame = await render([hello], { originX: 50, originY: 55, zoom: 12 });

    const inked = ink(frame, [0, 400], [0, 400]);
    let between = 0;
    for (let y = 0; y < 400; y++) {
      for (let x = 0; x < 400; x++) {
        const red = redAt(frame, x, y);
        if (red >= 60 && red <= 220) between++;
      }
    }
    expect(inked, "the glyph is big on screen").toBeGreaterThan(5_000);
    expect(
      between * 4,
      `a thin fringe of anti-aliasing, not a blur: ${between} of ${inked}`,
    ).toBeLessThan(inked);
  });

  it("leaves a gap in an arrow for its label", async () => {
    // A straight arrow along y = 200 with a "." label at its middle
    // (200, 200): the gap is the label's box and 5 around, so at least 7
    // either side of the middle, where the dot is not.
    const arrow = `{"id":"a","type":"arrow","x":50,"y":200,"width":300,"height":0,"points":[[0,0],[300,0]],"roughness":0,"strokeWidth":2,"endArrowhead":null,"boundElements":[{"type":"text","id":"t"}]}`;
    const label = textElement(
      "t",
      0,
      0,
      [5, 25],
      ".",
      `,"containerId":"a","textAlign":"center","verticalAlign":"middle"`,
    );

    const frame = await render([arrow, label]);

    expect(redAt(frame, 150, 200), "the arrow away from its label").toBeLessThan(60);
    expect(redAt(frame, 194, 200), "left of the dot").toBe(255);
    expect(redAt(frame, 206, 200), "right of the dot").toBe(255);
  });

  it("shows a character the font lacks as a box of ink", async () => {
    const frame = await render([
      textElement("t", 50, 50, [100, 50], "日", `,"fontSize":40,"lineHeight":1.25`),
    ]);

    expect(ink(frame, [50, 100], [50, 100])).toBeGreaterThan(100);
  });

  it("is not rebuilt while nothing changes", async () => {
    const renderer = harness.makeRenderer();
    renderer.setGeometry(harness.scenes.geometryOf(drawing(hello)));
    await harness.draw(renderer);
    const built = renderer.elementsBuilt;

    renderer.viewport = { originX: 10, originY: 10, zoom: 1 };
    await harness.draw(renderer);

    expect(built).toBe(1);
    expect(renderer.elementsBuilt, "panning keeps the glyph meshes").toBe(built);
  });
});

describe("text reference images", () => {
  /** Text in every alignment, size and family, rotated and translucent, and labels in shapes. */
  it("draws text", async () => {
    const image = await harness.render(
      harness.scenes.text(),
      { originX: 0, originY: 0, zoom: 1 },
      { size: 520 },
    );
    await expectMatchesReference(image, "text");
  });

  /** The same text three times as large: glyph outlines, not bitmaps. */
  it("draws text zoomed", async () => {
    const image = await harness.render(
      harness.scenes.text(),
      { originX: 250, originY: 20, zoom: 3 },
      { size: 600 },
    );
    await expectMatchesReference(image, "text-zoomed");
  });
});
