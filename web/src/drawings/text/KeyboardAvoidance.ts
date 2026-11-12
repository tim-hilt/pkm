import { TextAlign, type Viewport } from "../../core/Core";

/** A rectangle in the Canvas's coordinates. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Room kept between the caret's line and the keyboard or the Canvas's edge, in points. */
export const keyboardMargin = 16;

/**
 * Keeps the text being typed visible above the on-screen keyboard: given where
 * the keyboard covers the Canvas and where the text and its caret are, says how
 * the core's viewport must move (the Scene and the Text view then move
 * together). A port of the Swift `KeyboardAvoidance`.
 *
 * Moves `text` to the middle of the room between `top` (the top of the visible
 * page) and the keyboard, or the caret's line if the text is too tall for it.
 * `undefined` if the caret's line is clear of the keyboard already, or if no
 * keyboard covers it (a hardware keyboard, or a floating one beside or above
 * it). Only panning, never zooming.
 */
export function textAboveKeyboard(
  keyboard: Rect | undefined,
  top: number,
  text: Rect,
  caret: Rect,
  current: Viewport,
): Viewport | undefined {
  if (!keyboard || current.zoom <= 0) return undefined;
  const line: Rect = {
    x: caret.x,
    y: caret.y,
    width: Math.max(caret.width, 1),
    height: caret.height + keyboardMargin,
  };
  if (!intersects(line, keyboard)) return undefined;
  const room = keyboard.y - top;
  const shown = text.height + 2 * keyboardMargin <= room ? text : caret;
  const lift = shown.y + shown.height / 2 - (top + room / 2);
  if (lift <= 0) return undefined;
  return { ...current, originY: current.originY + lift / current.zoom };
}

/**
 * Follows the caret out of view as Excalidraw does: the viewport that pans
 * `caret` just into `room` (what is visible of the Canvas), with the margin to
 * spare, or `undefined` if it is in already. A port of the Swift
 * `KeyboardAvoidance.follow`. Only panning, never zooming.
 */
export function caretInView(room: Rect, caret: Rect, current: Viewport): Viewport | undefined {
  if (current.zoom <= 0) return undefined;
  const dx = overflow(caret.x, caret.x + caret.width, room.x, room.x + room.width);
  const dy = overflow(caret.y, caret.y + caret.height, room.y, room.y + room.height);
  if (dx === 0 && dy === 0) return undefined;
  return {
    ...current,
    originX: current.originX + dx / current.zoom,
    originY: current.originY + dy / current.zoom,
  };
}

/** How far `from...to` must move to be within `min...max`, the margin in. */
function overflow(from: number, to: number, min: number, max: number): number {
  if (to > max - keyboardMargin) return to - (max - keyboardMargin);
  if (from < min + keyboardMargin) return from - (min + keyboardMargin);
  return 0;
}

function intersects(a: Rect, b: Rect): boolean {
  if (a.width <= 0 || a.height <= 0 || b.width <= 0 || b.height <= 0) return false;
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/**
 * Where the on-screen keyboard covers the Canvas, from the visual viewport (it
 * shrinks when the keyboard comes up while the layout viewport stays), in the
 * Canvas's coordinates; `undefined` if no keyboard does. Pinch-zooming the
 * page also shrinks it a little, which is below `minimum`.
 */
export function keyboardCovering(
  canvas: Rect,
  visual: { offsetTop: number; height: number },
  layoutHeight: number,
  minimum = 100,
): Rect | undefined {
  const top = visual.offsetTop + visual.height;
  const height = layoutHeight - top;
  if (height < minimum) return undefined;
  return { x: 0, y: top - canvas.y, width: canvas.width, height };
}

const alignShares: Record<TextAlign, number> = {
  [TextAlign.Left]: 0,
  [TextAlign.Center]: 0.5,
  [TextAlign.Right]: 1,
};

/**
 * The caret in a text view: on the line of `value` that `caret` (an offset) is
 * on, counted by the line breaks typed, after the characters before it on that
 * line, as wide as `lineWidth` says, in the box of the text.
 */
export function caretLine(
  box: {
    x: number;
    y: number;
    width: number;
    height: number;
    lineHeight: number;
    align: TextAlign;
  },
  value: string,
  caret: number,
  lineWidth: (text: string) => number,
): Rect {
  const before = value.slice(0, caret).split("\n");
  const line = before.length - 1;
  const lines = value.split("\n");
  // Wrapped text has more lines than typed: counted from the bottom at the end.
  const y =
    caret >= value.length
      ? box.y + box.height - box.lineHeight
      : box.y + Math.min(line, lines.length - 1) * box.lineHeight;
  const start = (box.width - lineWidth(lines[line])) * alignShares[box.align];
  const x = Math.min(Math.max(start, 0) + lineWidth(before[line]), box.width);
  return { x: box.x + x, y, width: 1, height: box.lineHeight };
}
