import { describe, expect, it } from "vitest";
import { TextAlign } from "../../core/Core";
import {
  type Rect,
  caretInView,
  caretLine,
  keyboardCovering,
  textAboveKeyboard,
} from "./KeyboardAvoidance";

// iPhone 15 Pro: the keyboard covers y 510...852 of a 393 × 852 Canvas, below
// the status bar's 59 points. The room's middle is at 284.5.
const viewport = { originX: 10, originY: 20, zoom: 2 };
const keyboard: Rect = { x: 0, y: 510, width: 393, height: 342 };
const top = 59;
const caret = (bottom: number): Rect => ({ x: 50, y: bottom - 20, width: 2, height: 20 });
const text = (from: number, to: number): Rect => ({
  x: 40,
  y: from,
  width: 120,
  height: to - from,
});
const avoid = (shown: Rect, caretBottom: number, over: Rect | undefined = keyboard) =>
  textAboveKeyboard(over, top, shown, caret(caretBottom), viewport);

describe("keeping the text above the keyboard", () => {
  it("moves a covered text to the middle of the room above the keyboard", () => {
    // Middle 580 → 284.5: 295.5 points up = 147.75 Scene units.
    expect(avoid(text(560, 600), 600)).toEqual({ originX: 10, originY: 167.75, zoom: 2 });
  });

  it("moves the text for a caret in the margin above the keyboard", () => {
    // Middle 490 → 284.5: 205.5 points up.
    expect(avoid(text(480, 500), 500)).toEqual({ originX: 10, originY: 122.75, zoom: 2 });
  });

  it("centres the caret's line of a text taller than the room", () => {
    // Caret middle 590 → 284.5: 305.5 points up.
    expect(avoid(text(100, 600), 600)).toEqual({ originX: 10, originY: 172.75, zoom: 2 });
  });

  it("does not pan a caret that is clear of the keyboard", () => {
    expect(avoid(text(474, 494), 494)).toBeUndefined();
    expect(avoid(text(80, 100), 100)).toBeUndefined();
  });

  it("does not pan without a keyboard over the Canvas", () => {
    expect(textAboveKeyboard(undefined, top, text(780, 800), caret(800), viewport)).toBeUndefined();
    expect(avoid(text(780, 800), 800, { x: 0, y: 0, width: 0, height: 0 })).toBeUndefined();
  });

  it("does not pan for a floating keyboard beside or above the caret", () => {
    const floating: Rect = { x: 200, y: 300, width: 180, height: 120 };
    expect(avoid(text(380, 400), 400, floating)).toBeUndefined();
    const below: Rect = { x: 250, y: 600, width: 2, height: 20 };
    expect(textAboveKeyboard(floating, top, below, below, viewport)).toBeUndefined();
  });
});

describe("finding the keyboard from the visual viewport", () => {
  const canvas: Rect = { x: 0, y: 0, width: 393, height: 852 };

  it("is what the visual viewport leaves of the layout viewport", () => {
    expect(keyboardCovering(canvas, { offsetTop: 0, height: 510 }, 852)).toEqual({
      x: 0,
      y: 510,
      width: 393,
      height: 342,
    });
  });

  it("is in the Canvas's coordinates", () => {
    expect(
      keyboardCovering(
        { x: 0, y: 50, width: 393, height: 802 },
        { offsetTop: 0, height: 510 },
        852,
      ),
    ).toEqual({ x: 0, y: 460, width: 393, height: 342 });
  });

  it("is none while the visual viewport fills the layout viewport, or is just a little smaller", () => {
    expect(keyboardCovering(canvas, { offsetTop: 0, height: 852 }, 852)).toBeUndefined();
    expect(keyboardCovering(canvas, { offsetTop: 0, height: 800 }, 852)).toBeUndefined();
  });
});

describe("following the caret out of view", () => {
  // A 800 × 600 Canvas below a 20 point status bar.
  const room: Rect = { x: 0, y: 20, width: 800, height: 580 };
  const at = (x: number, y: number): Rect => ({ x, y, width: 1, height: 20 });

  it("pans a caret past the right edge just into view, with the margin", () => {
    // Right edge 801 must reach 800 − 16 = 784: 17 points = 8.5 Scene units.
    expect(caretInView(room, at(800, 100), viewport)).toEqual({
      originX: 18.5,
      originY: 20,
      zoom: 2,
    });
  });

  it("pans a caret past the left edge or above the top", () => {
    expect(caretInView(room, at(4, 100), viewport)).toEqual({ originX: 4, originY: 20, zoom: 2 });
    expect(caretInView(room, at(100, 10), viewport)).toEqual({ originX: 10, originY: 7, zoom: 2 });
  });

  it("pans a caret past the bottom edge", () => {
    // Bottom 610 must reach 600 − 16 = 584: 26 points up.
    expect(caretInView(room, at(100, 590), viewport)).toEqual({
      originX: 10,
      originY: 33,
      zoom: 2,
    });
  });

  it("does not pan a caret in view", () => {
    expect(caretInView(room, at(783, 563), viewport)).toBeUndefined();
    expect(caretInView(room, at(16, 36), viewport)).toBeUndefined();
  });
});

describe("where the caret is", () => {
  // Each character is 10 points wide.
  const width = (text: string) => text.length * 10;
  const box = (align = TextAlign.Left) => ({
    x: 100,
    y: 200,
    width: 60,
    height: 70,
    lineHeight: 35,
    align,
  });

  it("is the last line of the box with the caret at the end", () => {
    expect(caretLine(box(), "ab\ncd", 5, width)).toEqual({ x: 120, y: 235, width: 1, height: 35 });
  });

  it("is the line the caret is on, after the characters before it", () => {
    expect(caretLine(box(), "ab\ncd", 1, width)).toEqual({ x: 110, y: 200, width: 1, height: 35 });
  });

  it("follows the alignment of a line shorter than the box", () => {
    expect(caretLine(box(TextAlign.Center), "ab\ncd", 5, width).x).toBe(140);
    expect(caretLine(box(TextAlign.Right), "ab\ncd", 5, width).x).toBe(160);
  });

  it("stays in the box when a wrapped line is longer than it", () => {
    expect(caretLine(box(), "abcdefghij", 10, width).x).toBe(160);
  });
});
