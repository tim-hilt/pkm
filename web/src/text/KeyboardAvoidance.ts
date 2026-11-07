import type { Viewport } from "../core/Core";

/** A rectangle in the Canvas's coordinates. */
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Room kept between the caret's line and the keyboard, in points. */
export const keyboardMargin = 16;

/**
 * Keeps the line being typed above the on-screen keyboard: given where the
 * keyboard covers the Canvas and where the caret is, says how the core's
 * viewport must move (the Scene and the Text view then move together). A port
 * of the Swift `KeyboardAvoidance`.
 *
 * `undefined` if the caret is clear already, or if no keyboard covers it (a
 * hardware keyboard, or a floating one beside or above it). Only panning,
 * never zooming.
 */
export function caretClearOf(
  keyboard: Rect | undefined,
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
  const lift = line.y + line.height - keyboard.y;
  return { ...current, originY: current.originY + lift / current.zoom };
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

/**
 * The caret's line of a text view: the line of `value` that `caret` (an offset)
 * is on, counted by the line breaks typed, in the box of the text.
 */
export function caretLine(
  box: { x: number; y: number; height: number; lineHeight: number },
  value: string,
  caret: number,
): Rect {
  const line = value.slice(0, caret).split("\n").length - 1;
  const lines = value.split("\n").length;
  // Wrapped text has more lines than typed: counted from the bottom at the end.
  const y =
    caret >= value.length
      ? box.y + box.height - box.lineHeight
      : box.y + Math.min(line, lines - 1) * box.lineHeight;
  return { x: box.x, y, width: 1, height: box.lineHeight };
}
