import { describe, expect, it } from "vitest";
import { menuCorner } from "./CardMenu";

// A 200 × 240 card at (100, 50) in the viewport, a 140 × 120 menu.
const card = { left: 100, top: 50, width: 200, height: 240 };
const menu = { width: 140, height: 120 };

describe("where a card's menu opens", () => {
  it("has its corner at the press, in the card's coordinates", () => {
    expect(menuCorner({ x: 120, y: 70 }, card, menu)).toEqual({ left: 20, top: 20 });
  });

  it("is kept within the card near its right and bottom edges", () => {
    expect(menuCorner({ x: 290, y: 280 }, card, menu)).toEqual({ left: 60, top: 120 });
  });

  it("starts at the card's corner when the menu is bigger than the card", () => {
    expect(menuCorner({ x: 150, y: 60 }, card, { width: 300, height: 300 })).toEqual({
      left: 0,
      top: 0,
    });
  });
});
