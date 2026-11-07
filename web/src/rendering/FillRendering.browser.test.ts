import { beforeAll, describe, expect, it } from "vitest";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";
import { pentagram } from "../testing/TestScenes";
import {
  type Point,
  fillPart,
  fillSubpaths,
  handBuiltGeometry,
  strokePart,
} from "./HandBuiltGeometry";
import { GeometryFillRule } from "./GeometryBuffers";
import { type RenderedImage, redAt } from "./RenderedImage";

/**
 * Renders fills offscreen and probes pixels, a port of the Swift
 * `FillRenderingTests`. Fill colour #a5d8ff has a red channel of 165, paper
 * is 255 and ink 30, so the red channel tells them apart.
 */

const fillRed = 165;
let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

const render = (geometry: Parameters<RenderHarness["render"]>[0]) => harness.render(geometry);
const red = (frame: RenderedImage, x: number, y: number) => redAt(frame, x, y);
const within = (value: number, low: number, high: number) => value >= low && value <= high;

describe("fills from the core", () => {
  const filledRectangle = `,"backgroundColor":"#a5d8ff","fillStyle":"solid"`;

  it("fills the shape with a solid background and nothing else", async () => {
    const frame = await render(harness.scenes.rectangle(filledRectangle));

    // Rectangle (50,100)-(330,280).
    expect(red(frame, 200, 190), "inside").toBe(fillRed);
    expect(red(frame, 70, 120), "near the corner").toBe(fillRed);
    expect(red(frame, 20, 20), "outside").toBe(255);
    expect(red(frame, 200, 295), "below").toBe(255);
    expect(red(frame, 200, 100), "the outline lies over the fill").toBeLessThan(60);
  });

  it("takes the fill colour from the background colour", async () => {
    const frame = await render(
      harness.scenes.rectangle(`,"backgroundColor":"#e03131","fillStyle":"solid"`),
    );

    // #e03131: red 224.
    expect(red(frame, 200, 190)).toBe(224);
  });

  it("leaves the inside paper for a transparent background", async () => {
    const frame = await render(harness.scenes.rectangle(`,"backgroundColor":"transparent"`));

    expect(red(frame, 200, 190)).toBe(255);
  });

  it("makes fills as opaque as the Element", async () => {
    const frame = await render(harness.scenes.rectangle(filledRectangle + `,"opacity":50`));

    // Half of 165 over half of paper 255, blended once.
    expect(within(red(frame, 200, 190), 205, 215), `${red(frame, 200, 190)}`).toBe(true);
  });

  it("hatches with lines of the background colour and paper between", async () => {
    const frame = await render(
      harness.scenes.rectangle(`,"backgroundColor":"#a5d8ff","fillStyle":"hachure"`),
    );

    // Lines every 8 units of 1 unit thickness: some pixels in a row are fill
    // coloured, some are paper, none are ink.
    const row = Array.from({ length: 181 }, (_, i) => red(frame, 100 + i, 190));
    expect(
      row.some((value) => value < 200),
      "has line pixels",
    ).toBe(true);
    expect(row, "has paper between the lines").toContain(255);
    expect(
      row.every((value) => value >= 120),
      "no ink",
    ).toBe(true);
  });
});

describe("winding rules", () => {
  const star = pentagram(200, 180);

  it("fills concave outlines only inside", async () => {
    // An L: the notch at the top right stays paper.
    const frame = await render(
      handBuiltGeometry(
        fillPart([
          [50, 50],
          [150, 50],
          [150, 250],
          [350, 250],
          [350, 350],
          [50, 350],
        ]),
      ),
    );

    expect(red(frame, 100, 100), "upright").toBe(fillRed);
    expect(red(frame, 250, 300), "foot").toBe(fillRed);
    expect(red(frame, 250, 100), "the notch").toBe(255);
    expect(red(frame, 320, 200), "above the foot").toBe(255);
  });

  it("fills the middle of a star by non-zero and leaves it empty by even-odd", async () => {
    const nonZero = await render(
      handBuiltGeometry(fillPart(star, { fillRule: GeometryFillRule.nonZero })),
    );
    const evenOdd = await render(
      handBuiltGeometry(fillPart(star, { fillRule: GeometryFillRule.evenOdd })),
    );

    for (const [name, frame] of [
      ["non-zero", nonZero],
      ["even-odd", evenOdd],
    ] as const) {
      expect(red(frame, 200, 40), `${name}: the top point`).toBe(fillRed);
      expect(red(frame, 20, 20), `${name}: outside`).toBe(255);
    }
    expect(red(nonZero, 200, 200), "non-zero: the middle is wound twice, inside").toBe(fillRed);
    expect(red(evenOdd, 200, 200), "even-odd: the middle is a hole").toBe(255);
  });

  it("makes opposing subpaths a hole only under non-zero", async () => {
    // Outer square clockwise (y down); the inner one counter-clockwise
    // cancels it out, the inner one in the same direction does not.
    const outer: Point[] = [
      [50, 50],
      [350, 50],
      [350, 350],
      [50, 350],
    ];
    const opposite: Point[] = [
      [150, 150],
      [150, 250],
      [250, 250],
      [250, 150],
    ];
    const same = [...opposite].reverse();

    const hole = await render(handBuiltGeometry(fillSubpaths([outer, opposite])));
    const filled = await render(handBuiltGeometry(fillSubpaths([outer, same])));
    const evenOddFilled = await render(
      handBuiltGeometry(fillSubpaths([outer, same], { fillRule: GeometryFillRule.evenOdd })),
    );

    expect(red(hole, 200, 200), "opposite direction: hole").toBe(255);
    expect(red(hole, 100, 100), "opposite direction: ring").toBe(fillRed);
    expect(red(filled, 200, 200), "same direction: filled").toBe(fillRed);
    expect(red(evenOddFilled, 200, 200), "even-odd: hole either way").toBe(255);
  });

  it("blends a translucent fill once where it overlaps itself", async () => {
    // The star's middle is wound twice; it must not look darker than the tips.
    const frame = await render(handBuiltGeometry(fillPart(star, { opacity: 0.5 })));

    const tip = red(frame, 200, 40);
    expect(within(tip, 205, 215), `${tip}`).toBe(true);
    expect(red(frame, 200, 200), "the middle").toBe(tip);
  });

  it("leaves out the hole of a part", async () => {
    const frame = await render(
      handBuiltGeometry(
        fillPart(
          [
            [50, 50],
            [350, 50],
            [350, 350],
            [50, 350],
          ],
          { holeX: 150, holeY: 150, holeWidth: 100, holeHeight: 100 },
        ),
      ),
    );

    expect(red(frame, 200, 200), "in the hole").toBe(255);
    expect(red(frame, 100, 100), "around it").toBe(fillRed);
  });
});

describe("fills and strokes together", () => {
  const triangle: Point[] = [
    [50, 50],
    [350, 50],
    [200, 350],
  ];

  it("paints a fill over the marks of earlier translucent strokes", async () => {
    // The translucent stroke leaves stencil marks; the triangle drawn after
    // it must be filled completely, also where they are.
    const frame = await render(
      handBuiltGeometry(
        strokePart(
          [
            [50, 200],
            [350, 200],
          ],
          { width: 40, opacity: 0.5 },
        ),
        fillPart(triangle),
      ),
    );

    expect(red(frame, 200, 200), "over the stroke").toBe(fillRed);
    expect(red(frame, 200, 100), "beside it").toBe(fillRed);
    expect(red(frame, 100, 300), "outside").toBe(255);
  });

  it("does not leak the marks of translucent strokes into a later fill after another fill", async () => {
    // The fill in between lies away from the stroke; the triangle after it
    // must still count its winding from zero where the stroke was.
    const elsewhere: Point[] = [
      [300, 300],
      [350, 300],
      [350, 350],
      [300, 350],
    ];
    const lowerLeft: Point[] = [
      [50, 50],
      [50, 350],
      [350, 350],
    ];
    const frame = await render(
      handBuiltGeometry(
        strokePart(
          [
            [60, 100],
            [340, 100],
          ],
          { width: 40, opacity: 0.5 },
        ),
        fillPart(elsewhere),
        fillPart(lowerLeft),
      ),
    );

    // (300, 100) is on the stroke, inside the triangle's cover rectangle but
    // outside the triangle: half-opaque ink over paper.
    expect(
      within(red(frame, 300, 100), 135, 150),
      `outside the triangle: ${red(frame, 300, 100)}`,
    ).toBe(true);
    expect(red(frame, 100, 300), "inside the triangle").toBe(fillRed);
  });

  it("draws every translucent stroke however many came before", async () => {
    // More translucent strokes than the stencil has values: the last one
    // crosses the first and must blend over it like any other Element.
    const filler = Array.from({ length: 300 }, () =>
      strokePart(
        [
          [10, 380],
          [12, 380],
        ],
        { width: 1, opacity: 0.5 },
      ),
    );
    const first = strokePart(
      [
        [50, 200],
        [350, 200],
      ],
      { width: 40, opacity: 0.5 },
    );
    const last = strokePart(
      [
        [200, 50],
        [200, 350],
      ],
      { width: 40, opacity: 0.5 },
    );
    for (const count of [254, 255, 256]) {
      const frame = await render(handBuiltGeometry(first, ...filler.slice(0, count), last));
      // Ink at half opacity twice over paper: 255 -> 142 -> 86.
      expect(within(red(frame, 200, 200), 80, 92), `after ${count}: ${red(frame, 200, 200)}`).toBe(
        true,
      );
    }
  });

  it("draws a translucent stroke fully over an earlier fill", async () => {
    const frame = await render(
      handBuiltGeometry(
        fillPart(triangle),
        strokePart(
          [
            [50, 200],
            [350, 200],
          ],
          { width: 40, opacity: 0.5 },
        ),
      ),
    );

    // Ink at half opacity over the fill: (30 + 165) / 2 = 97.
    expect(within(red(frame, 200, 200), 92, 102), `${red(frame, 200, 200)}`).toBe(true);
    expect(within(red(frame, 360, 200), 135, 150), `over paper: ${red(frame, 360, 200)}`).toBe(
      true,
    );
  });

  it("keeps consecutive fills from leaking into each other", async () => {
    // Two overlapping squares with the opposite winding; the second's
    // stencil pass must start from zero where the first one was.
    const a: Point[] = [
      [50, 50],
      [250, 50],
      [250, 250],
      [50, 250],
    ];
    const b: Point[] = [
      [150, 150],
      [150, 350],
      [350, 350],
      [350, 150],
    ];
    const frame = await render(handBuiltGeometry(fillPart(a, { color: 0xe03131ff }), fillPart(b)));

    expect(red(frame, 100, 100), "only the first").toBe(224);
    expect(red(frame, 200, 200), "the second over the first").toBe(fillRed);
    expect(red(frame, 300, 300), "only the second").toBe(fillRed);
  });

  it("caches fills like strokes and re-flattens when zooming to another level", async () => {
    const renderer = harness.makeRenderer();
    renderer.setGeometry(
      handBuiltGeometry(
        fillPart([
          [50, 50],
          [350, 50],
          [200, 350],
        ]),
      ),
    );
    await harness.draw(renderer);
    renderer.viewport = { originX: 30, originY: 30, zoom: 1 };
    await harness.draw(renderer);
    expect(renderer.elementsBuilt, "panned").toBe(1);

    renderer.viewport = { originX: 30, originY: 30, zoom: 4 };
    await harness.draw(renderer);
    expect(renderer.elementsBuilt, "zoomed in").toBe(2);
  });
});
