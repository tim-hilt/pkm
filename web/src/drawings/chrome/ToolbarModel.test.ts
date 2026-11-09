import { describe, expect, it } from "vitest";
import { Tool } from "../../core/Core";
import { focusTarget, mobileToolSlots, shownTool, toolItems } from "./ToolbarModel";

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

describe("the phone toolbar's slots", () => {
  it("share Tools of a family in Excalidraw's phone order", () => {
    expect(mobileToolSlots.map((slot) => slot.tools)).toEqual([
      [Tool.Hand],
      [Tool.Selection],
      [Tool.Freedraw],
      [Tool.Eraser],
      [Tool.Rectangle, Tool.Diamond, Tool.Ellipse],
      [Tool.Arrow, Tool.Line],
      [Tool.Text],
    ]);
  });

  it("shows the armed Tool of its family, else the last chosen, else the default", () => {
    const shapes = mobileToolSlots[4];
    expect(shownTool(shapes, Tool.Ellipse, Tool.Diamond)).toBe(Tool.Ellipse);
    expect(shownTool(shapes, Tool.Hand, Tool.Diamond)).toBe(Tool.Diamond);
    expect(shownTool(shapes, Tool.Hand, Tool.Line)).toBe(Tool.Rectangle);
    expect(shownTool(shapes, Tool.Hand, undefined)).toBe(Tool.Rectangle);
  });
});
