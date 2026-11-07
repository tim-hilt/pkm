import { describe, expect, it } from "vitest";
import { makeImage } from "./GeometryBuffers";
import { placeholderFills, scenePoint } from "./ImagePlaceholder";

describe("placeholderFills", () => {
  it("is a grey box with the icon over it, in Excalidraw's colours", () => {
    const [box, ...icon] = placeholderFills(100, 60, false);

    expect(box.color).toBe(0xe7e7e7ff);
    expect(box.path.coords).toEqual([0, 0, 100, 0, 100, 60, 0, 60]);
    expect(icon).toHaveLength(1);
    expect(icon[0].color).toBe(0x888888ff);
  });

  it("is darker on the dark Canvas", () => {
    expect(placeholderFills(100, 60, false, true)[0].color).toBe(0x2e2e2eff);
  });

  it("centres an icon of 40 % of the shorter side in the box", () => {
    const [, icon] = placeholderFills(200, 100, false);
    const xs = icon.path.coords.filter((_, i) => i % 2 === 0);
    const ys = icon.path.coords.filter((_, i) => i % 2 === 1);

    // The artwork spans its 512 box, less 64 px above and below: 40 wide, 35 tall.
    expect(Math.min(...xs)).toBeCloseTo(80, 3);
    expect(Math.max(...xs)).toBeCloseTo(120, 3);
    expect(Math.min(...ys)).toBeCloseTo(30 + (64 / 512) * 40, 3);
    expect(Math.max(...ys)).toBeCloseTo(70 - (64 / 512) * 40, 3);
  });

  it("never makes the icon larger than 100 Scene units", () => {
    const [, icon] = placeholderFills(1000, 1000, false);
    const xs = icon.path.coords.filter((_, i) => i % 2 === 0);
    expect(Math.max(...xs) - Math.min(...xs)).toBeCloseTo(100, 3);
  });

  it("has two pieces of artwork when failed, and is only the box when there is no room for an icon", () => {
    expect(placeholderFills(100, 60, true)).toHaveLength(3);
    expect(placeholderFills(100, 0, false)).toHaveLength(1);
  });
});

describe("scenePoint", () => {
  it("is the point moved by the box's position if the image is plain", () => {
    expect(scenePoint(makeImage({ x: 10, y: 20 }), 5, 7)).toEqual([15, 27]);
  });

  it("mirrors about the box's centre", () => {
    const image = makeImage({ x: 10, y: 20, width: 100, height: 50, scaleX: -1 });
    expect(scenePoint(image, 0, 0)).toEqual([110, 20]);
  });

  it("turns clockwise about the centre", () => {
    const image = makeImage({ width: 100, height: 100, angle: Math.PI / 2 });
    const [x, y] = scenePoint(image, 0, 0);
    expect(x).toBeCloseTo(100, 9);
    expect(y).toBeCloseTo(0, 9);
  });
});
