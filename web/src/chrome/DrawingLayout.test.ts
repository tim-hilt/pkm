import { describe, expect, it } from "vitest";
import {
  bottomBarWidth,
  layoutFor,
  popoverPlacement,
  propertiesMaxHeight,
  propertiesTop,
} from "./DrawingLayout";

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

describe("the phone's bottom bar", () => {
  it.each([
    [393, 365], // iPhone portrait: the container less 28
    [478, 450],
    [852, 450], // capped
    [20, 0],
  ])("is %d wide in a container of that width: %d", (container, bar) => {
    expect(bottomBarWidth(container)).toBe(bar);
  });
});

describe("a phone popover's placement", () => {
  const container = { width: 393, height: 852 };
  const content = { width: 200, height: 150 };
  const anchor = (midX: number) => ({ x: midX - 16, y: 700, width: 32, height: 32 });

  it("is centred on its anchor, its bottom a gap above the bar", () => {
    const place = popoverPlacement(anchor(200), content, container, 690);
    expect(place.frame).toEqual({ x: 100, y: 690 - 12 - 150, width: 200, height: 150 });
    expect(place.pointerX).toBe(200);
  });

  it("keeps clear of the container's edges while the pointer stays on the anchor", () => {
    const left = popoverPlacement(anchor(30), content, container, 690);
    expect(left.frame.x).toBe(8);
    expect(left.pointerX).toBe(30);
    const right = popoverPlacement(anchor(380), content, container, 690);
    expect(right.frame.x + right.frame.width).toBe(385);
  });

  it("is capped to the room above the bar and to the width of the container", () => {
    const place = popoverPlacement(anchor(200), { width: 900, height: 900 }, container, 300);
    expect(place.maxHeight).toBe(300 - 12 - 8);
    expect(place.frame).toEqual({ x: 8, y: 300 - 12 - 280, width: 377, height: 280 });
  });
});
