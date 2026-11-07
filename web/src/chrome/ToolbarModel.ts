import { Tool } from "../core/Core";

/** One Tool button: the name that labels it for screen readers and as its tooltip, and its glyph. */
export interface ToolItem {
  tool: Tool;
  name: string;
  /** SVG markup inside a 24 x 24 view box, drawn with the current colour. */
  glyph: string;
}

/** The label of the Tool lock button, as in the native app. */
export const toolLockLabel = "Keep tool selected";

const hand = `<path d="M8 13V6.5a1.5 1.5 0 0 1 3 0V11m0-5.5v-1a1.5 1.5 0 0 1 3 0V11m0-5a1.5 1.5 0 0 1 3 0v7.5a6 6 0 0 1-6 6h-.5a6 6 0 0 1-5-2.7L4 14.5a1.5 1.5 0 0 1 2.4-1.7L8 14.5"/>`;
const pointer = `<path d="M6 4l12 7.5-5.2 1.3L15.5 19l-2.4 1-2.7-6.3L6 17z"/>`;
const rectangle = `<rect x="4.5" y="5.5" width="15" height="13" rx="1"/>`;
const diamond = `<path d="M12 4l8 8-8 8-8-8z"/>`;
const ellipse = `<circle cx="12" cy="12" r="7.5"/>`;
const arrow = `<path d="M5 12h14m-5-5l5 5-5 5"/>`;
const line = `<path d="M5 19L19 5"/>`;
const freedraw = `<path d="M4 20l1-4L16.5 4.5a2 2 0 0 1 3 3L8 19z M14.5 6.5l3 3"/>`;
const text = `<path d="M6 7V5h12v2M12 5v14m-3 0h6"/>`;
const eraser = `<path d="M9 19l-4.5-4.5a1.5 1.5 0 0 1 0-2L13 4a1.5 1.5 0 0 1 2 0l4.5 4.5a1.5 1.5 0 0 1 0 2L11 19zM8 11l6 6M9 19h10"/>`;

/** Excalidraw's toolbar island, in its order (after the Tool lock). */
export const toolItems: readonly ToolItem[] = [
  { tool: Tool.Hand, name: "Hand", glyph: hand },
  { tool: Tool.Selection, name: "Selection", glyph: pointer },
  { tool: Tool.Rectangle, name: "Rectangle", glyph: rectangle },
  { tool: Tool.Diamond, name: "Diamond", glyph: diamond },
  { tool: Tool.Ellipse, name: "Ellipse", glyph: ellipse },
  { tool: Tool.Arrow, name: "Arrow", glyph: arrow },
  { tool: Tool.Line, name: "Line", glyph: line },
  { tool: Tool.Freedraw, name: "Draw", glyph: freedraw },
  { tool: Tool.Text, name: "Text", glyph: text },
  { tool: Tool.Eraser, name: "Eraser", glyph: eraser },
];

export const lockGlyph = (locked: boolean): string =>
  locked
    ? `<rect x="6" y="11" width="12" height="9" rx="1.5"/><path d="M8.5 11V8a3.5 3.5 0 0 1 7 0v3"/>`
    : `<rect x="6" y="11" width="12" height="9" rx="1.5"/><path d="M8.5 11V8a3.5 3.5 0 0 1 6.8-1.2"/>`;

/** The index of the button that arrow keys, Home and End move focus to, or undefined for other keys. */
export function focusTarget(key: string, current: number, count: number): number | undefined {
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return (current + 1) % count;
    case "ArrowLeft":
    case "ArrowUp":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return undefined;
  }
}
