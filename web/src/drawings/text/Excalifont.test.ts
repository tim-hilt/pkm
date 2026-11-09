import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type GeometryTextLine, GeometryTextAlign } from "../rendering/GeometryBuffers";
import { Excalifont } from "./Excalifont";
import { textLineOutline } from "./TextLineOutline";

/**
 * The `TextMeasurer` the core gets and the glyph outlines the renderer draws,
 * both from the bundled Excalifont: a port of the Swift `ExcalifontTests`.
 * Expected numbers are read from the font file itself (hmtx and glyf tables),
 * not computed the way the code does.
 */
const ttf = readFileSync(new URL("../../../generated/Excalifont-Regular.ttf", import.meta.url));
const makeFont = () => new Excalifont(new Uint8Array(ttf));

let font: Excalifont;
beforeAll(() => {
  font = makeFont();
});

const xs = (coords: number[]) => coords.filter((_, i) => i % 2 === 0);
const ys = (coords: number[]) => coords.filter((_, i) => i % 2 === 1);

describe("measuring", () => {
  it("is as wide as the font says a glyph's advance is", () => {
    // Advances in the font's 1000 units per em: space 400, "l" 225.
    expect(font.lineWidth(" ", 1000)).toBe(400);
    expect(font.lineWidth("l", 1000)).toBe(225);
    expect(font.lineWidth("l", 20)).toBeCloseTo(4.5, 6);
  });

  it("grows with the length and the size of the text", () => {
    expect(font.lineWidth("", 20)).toBe(0);
    expect(font.lineWidth("ll", 20)).toBeGreaterThan(font.lineWidth("l", 20));
    expect(font.lineWidth("Hello", 40)).toBeCloseTo(4 * font.lineWidth("Hello", 10), 6);
  });

  it("applies the font's pair kerning", () => {
    // GPOS kern: a comma followed by a quote is 60 units (of 1000) closer than their advances say.
    const [comma, quote] = [font.lineWidth(",", 1000), font.lineWidth('"', 1000)];
    expect(font.lineWidth(',"', 1000)).toBeCloseTo(comma + quote - 60, 6);
    expect(font.lineWidth('"l', 1000), "a pair without kerning").toBe(quote + 225);
  });
});

describe("outlines", () => {
  it("are the font's own, with y pointing down", () => {
    const layout = font.layout("l");
    const outline = layout.glyphs[0].outline;

    // "l" spans x 52...171 and y -20...825 above the baseline (glyf table).
    expect(Math.min(...xs(outline.coords))).toBeCloseTo(52, 0);
    expect(Math.max(...xs(outline.coords))).toBeCloseTo(171, 0);
    expect(
      Math.min(...ys(outline.coords)),
      "the top of the glyph is above the baseline",
    ).toBeCloseTo(-825, 0);
    expect(Math.max(...ys(outline.coords))).toBeCloseTo(20, 0);
    expect(layout.width).toBe(225);
  });

  it("follow each other by their advance", () => {
    const layout = font.layout("l l");

    expect(layout.glyphs.length, "a space has no outline").toBe(2);
    expect(layout.glyphs[0].x).toBe(0);
    expect(layout.glyphs[1].x, 'after "l" and the space').toBe(225 + 400);
  });

  it("are made once per glyph", () => {
    const fresh = makeFont();

    fresh.layout("lll");
    expect(fresh.cachedGlyphCount).toBe(1);
    fresh.layout("llll");
    expect(fresh.cachedGlyphCount, "drawn again, not made again").toBe(1);
    fresh.layout("H");
    expect(fresh.cachedGlyphCount).toBe(2);
  });

  it("show a character the font lacks as a hollow box, as wide as the missing glyph's advance", () => {
    const layout = font.layout("日");

    expect(layout.glyphs.length).toBe(1);
    const { outline } = layout.glyphs[0];
    expect(outline).toEqual(font.layout("\u{10FFFF}").glyphs[0].outline);
    // Excalifont's own .notdef is blank (advance 866), and the glyph after
    // it is a letter (U+0462), not a box: an outer and an inner rectangle.
    expect(outline.verbs).toEqual([0, 1, 1, 1, 0, 1, 1, 1]);
    expect(Math.min(...xs(outline.coords))).toBeGreaterThan(0);
    expect(Math.max(...xs(outline.coords))).toBeLessThan(866);
    expect(Math.max(...ys(outline.coords)), "stands on the baseline").toBe(0);
    expect(layout.width).toBe(866);
  });

  it("have every verb the operands it needs", () => {
    const { verbs, coords } = font
      .layout("Hello, World! 0123 &@")
      .glyphs.map((g) => g.outline)
      .reduce((all, o) => ({
        verbs: [...all.verbs, ...o.verbs],
        coords: [...all.coords, ...o.coords],
      }));

    const operands = verbs.reduce((sum, verb) => sum + [2, 2, 6][verb], 0);
    expect(coords.length).toBe(operands);
    expect(coords.every(Number.isFinite)).toBe(true);
  });
});

describe("a line of text", () => {
  const line: GeometryTextLine = {
    element: 0,
    version: 1,
    partIndex: 0,
    color: 0,
    opacity: 1,
    fontSize: 100,
    fontFamily: 5,
    align: GeometryTextAlign.left,
    x: 500,
    y: 300,
    angle: 0,
    textStart: 0,
    textLength: 1,
  };

  it("is one path in Scene coordinates", () => {
    // "l" drawn at size 100 with its anchor at (500, 300), left-aligned: x
    // from 500 + 5.2 to 500 + 17.1 (design 52...171 / 10).
    const path = textLineOutline(line, "l", font);

    expect(Math.min(...xs(path.coords))).toBeCloseTo(505.2, 1);
    expect(Math.max(...xs(path.coords))).toBeCloseTo(517.1, 1);
    expect(Math.min(...ys(path.coords))).toBeCloseTo(300 - 82.5, 1);
    expect(Math.max(...ys(path.coords))).toBeCloseTo(302, 1);
  });

  it("ends at the anchor when right-aligned", () => {
    // The line's advance is 22.5.
    const path = textLineOutline({ ...line, align: GeometryTextAlign.right }, "l", font);

    expect(Math.min(...xs(path.coords))).toBeCloseTo(500 - 22.5 + 5.2, 1);
  });

  it("is centred on the anchor when centre-aligned", () => {
    const path = textLineOutline({ ...line, align: GeometryTextAlign.center }, "l", font);

    expect(Math.min(...xs(path.coords))).toBeCloseTo(500 - 11.25 + 5.2, 1);
  });

  it("is turned clockwise about the anchor", () => {
    // A quarter turn: "up" points right.
    const path = textLineOutline({ ...line, angle: Math.PI / 2 }, "l", font);

    expect(Math.max(...xs(path.coords)), "the top of the glyph is right of the anchor").toBeCloseTo(
      582.5,
      0,
    );
  });
});
