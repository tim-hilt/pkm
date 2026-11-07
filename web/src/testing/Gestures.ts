import {
  type CorePointerEvent,
  type Editor,
  PointerButton,
  PointerKind,
  PointerPhase,
} from "../core/Core";

const noModifiers = { shift: false, ctrl: false, alt: false, meta: false };
let clock = 0;

/** One step of pointer 1 at `[x, y]`; time moves 10 ms per step. */
export function step(
  phase: PointerPhase,
  [x, y]: [number, number],
  kind = PointerKind.Mouse,
  pressure = 0.5,
  modifiers = noModifiers,
): CorePointerEvent {
  return {
    id: 1,
    phase,
    x,
    y,
    kind,
    button: PointerButton.Primary,
    pressure,
    timestamp: (clock += 0.01),
    modifiers,
  };
}

/** A pointer going down at the first point, through the others, and up at the last. */
export function drag(
  editor: Editor,
  points: [number, number][],
  kind = PointerKind.Mouse,
  pressure = 0.5,
  modifiers = noModifiers,
): void {
  editor.pointer(step(PointerPhase.Down, points[0], kind, pressure, modifiers));
  for (const point of points.slice(1)) {
    editor.pointer(step(PointerPhase.Move, point, kind, pressure, modifiers));
  }
  editor.pointer(step(PointerPhase.Up, points[points.length - 1], kind, pressure, modifiers));
}

/** A pointer going down at the first point and through the others, still down. */
export function press(editor: Editor, points: [number, number][]): void {
  editor.pointer(step(PointerPhase.Down, points[0]));
  for (const point of points.slice(1)) editor.pointer(step(PointerPhase.Move, point));
}
