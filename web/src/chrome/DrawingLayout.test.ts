import { describe, expect, it } from "vitest";
import { layoutFor, propertiesMaxHeight, propertiesTop } from "./DrawingLayout";

describe("the layout of a Drawing's container", () => {
  it.each([
    [1470, 832], // MacBook Air window
    [1024, 768], // small Mac window
    [1024, 1366], // iPad Pro portrait
    [1366, 1024], // iPad Pro landscape
    [678, 1024], // iPad Split View half
  ])("gives %d x %d the desktop layout", (width, height) => {
    expect(layoutFor(width, height)).toBe("desktop");
  });

  it.each([
    [393, 852], // iPhone portrait
    [852, 393], // iPhone landscape
    [599, 1024], // narrow Split View
  ])("gives %d x %d the mobile layout", (width, height) => {
    expect(layoutFor(width, height)).toBe("mobile");
  });
});

describe("the desktop Properties panel's placement", () => {
  it("leaves room for the controls", () => {
    expect(propertiesMaxHeight(832)).toBe(666);
    expect(propertiesMaxHeight(100)).toBe(0);
  });

  // The desktop toolbar's measured width is about 456.
  it.each([
    [1470, 16], // MacBook Air window
    [1366, 16], // iPad full screen
    [1024, 76], // 1024 pt Mac window
    [678, 76], // iPad Split View half
  ])("shares the toolbar's top in a %d wide container only if it fits (top %d)", (width, top) => {
    expect(propertiesTop(width, 456)).toBe(top);
  });

  it("gets room as it grows above the default top", () => {
    expect(propertiesMaxHeight(832, 16)).toBe(726);
  });
});
