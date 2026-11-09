import { describe, expect, it } from "vitest";
import { type Rect, caretClearOf, caretLine, keyboardCovering } from "./KeyboardAvoidance";

// iPhone 15 Pro: the keyboard covers y 510...852 of a 393 × 852 Canvas.
const viewport = { originX: 10, originY: 20, zoom: 2 };
const keyboard: Rect = { x: 0, y: 510, width: 393, height: 342 };
const caret = (bottom: number): Rect => ({ x: 50, y: bottom - 20, width: 2, height: 20 });

describe("keeping the caret above the keyboard", () => {
  it("pans a caret below the keyboard by the overlap plus the margin", () => {
    // Caret bottom 600 must reach 510 − 16 = 494: 106 points up = 53 Scene units.
    expect(caretClearOf(keyboard, caret(600), viewport)).toEqual({
      originX: 10,
      originY: 73,
      zoom: 2,
    });
  });

  it("pans a caret in the margin above the keyboard", () => {
    expect(caretClearOf(keyboard, caret(500), viewport)).toEqual({
      originX: 10,
      originY: 23,
      zoom: 2,
    });
  });

  it("pans further as the text grows", () => {
    const first = caretClearOf(keyboard, caret(520), viewport)!;
    const second = caretClearOf(keyboard, caret(494 + 30), first)!;
    expect(second.originY).toBe(first.originY + 15);
  });

  it("does not pan a caret that is clear of the keyboard", () => {
    expect(caretClearOf(keyboard, caret(494), viewport)).toBeUndefined();
    expect(caretClearOf(keyboard, caret(100), viewport)).toBeUndefined();
  });

  it("does not pan without a keyboard over the Canvas", () => {
    expect(caretClearOf(undefined, caret(800), viewport)).toBeUndefined();
    expect(caretClearOf({ x: 0, y: 0, width: 0, height: 0 }, caret(800), viewport)).toBeUndefined();
  });

  it("does not pan for a floating keyboard beside or above the caret", () => {
    const floating: Rect = { x: 200, y: 300, width: 180, height: 120 };
    expect(caretClearOf(floating, caret(400), viewport)).toBeUndefined();
    expect(
      caretClearOf(floating, { x: 250, y: 600, width: 2, height: 20 }, viewport),
    ).toBeUndefined();
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

describe("where the caret is", () => {
  const box = { x: 100, y: 200, height: 70, lineHeight: 35 };

  it("is the last line of the box with the caret at the end", () => {
    expect(caretLine(box, "ab\ncd", 5)).toEqual({ x: 100, y: 235, width: 1, height: 35 });
  });

  it("is the line the caret is on", () => {
    expect(caretLine(box, "ab\ncd", 1)).toEqual({ x: 100, y: 200, width: 1, height: 35 });
  });
});
