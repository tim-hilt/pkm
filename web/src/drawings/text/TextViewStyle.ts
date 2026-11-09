import { TextAlign, type TextEditor } from "../../core/Core";

/** Room on each side of the text, in points, for the caret at the end of a line. */
export const caretRoom = 6;

/** The CSS family the Text view is set in; see `loadExcalifontWebFont`. */
export const webFontFamily = "Excalifont";

/** The CSS properties that put the Text view where, and make it look as, the core says. */
export interface TextViewStyle {
  left: string;
  top: string;
  width: string;
  height: string;
  fontFamily: string;
  fontSize: string;
  lineHeight: string;
  textAlign: string;
  color: string;
  transform: string;
  whiteSpace: string;
}

const alignments: Record<TextAlign, string> = {
  [TextAlign.Left]: "left",
  [TextAlign.Center]: "center",
  [TextAlign.Right]: "right",
};

/**
 * The view is the box of the text, so the text sits where the core draws it
 * once the editing is over: the same Excalifont at the font size times the
 * zoom, lines of the same height, no padding but a little room beside the
 * text (a port of the Swift `TextEditorOverlay`'s placing).
 */
export function textViewStyle(editor: TextEditor): TextViewStyle {
  const red = (editor.color >>> 24) & 0xff;
  const green = (editor.color >>> 16) & 0xff;
  const blue = (editor.color >>> 8) & 0xff;
  const alpha = ((editor.color & 0xff) / 255) * editor.opacity;
  return {
    left: `${editor.x - caretRoom}px`,
    top: `${editor.y}px`,
    width: `${editor.width + 2 * caretRoom}px`,
    height: `${editor.height}px`,
    fontFamily: webFontFamily,
    fontSize: `${editor.fontSize}px`,
    lineHeight: `${editor.fontSize * editor.lineHeight}px`,
    textAlign: alignments[editor.align],
    color: `rgba(${red}, ${green}, ${blue}, ${alpha})`,
    // The core turns clockwise on the screen, as CSS does.
    transform: `rotate(${editor.angle}rad)`,
    whiteSpace: editor.fixedWidth ? "pre-wrap" : "pre",
  };
}
