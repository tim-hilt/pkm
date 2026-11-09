import { describe, expect, it } from "vitest";
import { TextAlign, type TextEditor } from "../../core/Core";
import { caretRoom, textViewStyle } from "./TextViewStyle";

const editor = (over: Partial<TextEditor> = {}): TextEditor => ({
  text: "Hello",
  x: 100,
  y: 187.5,
  width: 50,
  height: 25,
  fontSize: 20,
  lineHeight: 1.25,
  fontFamily: 5,
  align: TextAlign.Left,
  color: 0x1e1e1eff,
  opacity: 1,
  angle: 0,
  zoom: 1,
  fixedWidth: false,
  ...over,
});

describe("the style of the Text view", () => {
  it("is the box of the text with room for the caret on both sides", () => {
    const style = textViewStyle(editor());
    expect(style.left).toBe(`${100 - caretRoom}px`);
    expect(style.top).toBe("187.5px");
    expect(style.width).toBe(`${50 + 2 * caretRoom}px`);
    expect(style.height).toBe("25px");
  });

  it("sets the text in Excalifont at the font size and the exact line height", () => {
    const style = textViewStyle(editor({ fontSize: 28 }));
    expect(style.fontFamily).toBe("Excalifont");
    expect(style.fontSize).toBe("28px");
    // 28 points at a line height of 1.25.
    expect(style.lineHeight).toBe("35px");
  });

  it("follows zooming: the core reports the font size times the zoom", () => {
    expect(textViewStyle(editor({ fontSize: 22, zoom: 1.1 })).fontSize).toBe("22px");
  });

  it("aligns and colours the text as the Element is", () => {
    const style = textViewStyle(
      editor({ align: TextAlign.Center, color: 0xff000080, opacity: 0.5 }),
    );
    expect(style.textAlign).toBe("center");
    // 0x80 of 255 times the opacity.
    expect(style.color).toBe(`rgba(255, 0, 0, ${(128 / 255) * 0.5})`);
  });

  it("turns clockwise by the angle", () => {
    expect(textViewStyle(editor({ angle: Math.PI / 2 })).transform).toBe(
      `rotate(${Math.PI / 2}rad)`,
    );
  });

  it("wraps to the box only for a fixed width", () => {
    expect(textViewStyle(editor()).whiteSpace).toBe("pre");
    expect(textViewStyle(editor({ fixedWidth: true })).whiteSpace).toBe("pre-wrap");
  });
});
