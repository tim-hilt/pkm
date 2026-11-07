import { describe, expect, it } from "vitest";
import { Tool } from "../core/Core";
import { focusTarget, toolItems } from "./ToolbarModel";

describe("the toolbar's model", () => {
  it("offers every Tool of the native app in its order, each with a distinct label", () => {
    expect(toolItems.map((item) => item.name)).toEqual([
      "Hand",
      "Selection",
      "Rectangle",
      "Diamond",
      "Ellipse",
      "Arrow",
      "Line",
      "Draw",
      "Text",
      "Eraser",
    ]);
    expect(new Set(toolItems.map((item) => item.tool)).size).toBe(toolItems.length);
    expect(toolItems.find((item) => item.name === "Draw")?.tool).toBe(Tool.Freedraw);
  });

  it("moves focus with the arrow keys, wrapping at the ends", () => {
    expect(focusTarget("ArrowRight", 0, 5)).toBe(1);
    expect(focusTarget("ArrowRight", 4, 5)).toBe(0);
    expect(focusTarget("ArrowLeft", 0, 5)).toBe(4);
    expect(focusTarget("Home", 3, 5)).toBe(0);
    expect(focusTarget("End", 1, 5)).toBe(4);
    expect(focusTarget("a", 1, 5)).toBeUndefined();
  });
});
