import {
  type CoreMagnifyEvent,
  type CorePointerEvent,
  type CoreScrollEvent,
  type Modifiers,
  PointerButton,
  PointerKind,
  PointerPhase,
} from "../../core/Core";

// Translations from browser input to the core's. Units and coordinate systems
// only; what an input means is the core's business (ADR-0006). The Swift
// counterpart is the input half of CanvasView.swift and CanvasInput.swift.

/** The fields of a DOM `PointerEvent` that are forwarded. */
export interface PointerData extends ModifierData {
  pointerId: number;
  pointerType: string;
  clientX: number;
  clientY: number;
  pressure: number;
  /** Milliseconds. */
  timeStamp: number;
}

/** The fields of a DOM `WheelEvent` that are forwarded. */
export interface WheelData extends ModifierData {
  clientX: number;
  clientY: number;
  deltaX: number;
  deltaY: number;
  /** 0 pixels, 1 lines, 2 pages. */
  deltaMode: number;
}

interface ModifierData {
  shiftKey: boolean;
  ctrlKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

/** Where the Canvas is in the page: its top-left corner in client coordinates. */
export interface Origin {
  left: number;
  top: number;
}

/** Points a wheel line scrolls, as on the Swift Mac client. */
const pointsPerWheelLine = 10;

export const modifiersOf = (e: ModifierData): Modifiers => ({
  shift: e.shiftKey,
  ctrl: e.ctrlKey,
  alt: e.altKey,
  meta: e.metaKey,
});

export function pointerKind(pointerType: string): PointerKind {
  switch (pointerType) {
    case "pen":
      return PointerKind.Pencil;
    case "touch":
      return PointerKind.Finger;
    default:
      return PointerKind.Mouse;
  }
}

/** The button of a `PointerEvent.button`; those beyond the wheel button are the primary one's. */
export function pointerButton(button: number): PointerButton {
  switch (button) {
    case 1:
      return PointerButton.Middle;
    case 2:
      return PointerButton.Secondary;
    default:
      return PointerButton.Primary;
  }
}

/**
 * What a pointer's lost capture ends its press with, from `PointerEvent.buttons`.
 * A quick release can lose the capture before the pointerup arrives (and
 * the pointerup may then miss the Canvas): with no button down any more
 * that is the release. Lost while still pressed (the browser or the OS took
 * the pointer), the press is cancelled.
 */
export function lostCapturePhase(buttons: number): PointerPhase {
  return buttons === 0 ? PointerPhase.Up : PointerPhase.Cancel;
}

export function pointerEvent(
  e: PointerData,
  phase: PointerPhase,
  button: PointerButton,
  origin: Origin,
): CorePointerEvent {
  const kind = pointerKind(e.pointerType);
  return {
    id: e.pointerId,
    phase,
    x: e.clientX - origin.left,
    y: e.clientY - origin.top,
    kind,
    button,
    // Only a pen's pressure shapes strokes; the rest of the pointers get
    // the core's simulated pressure.
    pressure: kind === PointerKind.Pencil ? e.pressure : 0.5,
    timestamp: e.timeStamp / 1000,
    modifiers: modifiersOf(e),
  };
}

/**
 * A wheel or trackpad scroll. Browser deltas say where the content scrolls to
 * (down is positive); the core wants where the content moves, so they flip.
 * A trackpad pinch arrives as a wheel event with ctrl held, which the core
 * zooms with, as excalidraw.com does.
 */
export function scrollEvent(e: WheelData, origin: Origin, viewHeight: number): CoreScrollEvent {
  const scale = e.deltaMode === 1 ? pointsPerWheelLine : e.deltaMode === 2 ? viewHeight : 1;
  return {
    x: e.clientX - origin.left,
    y: e.clientY - origin.top,
    dx: -e.deltaX * scale,
    dy: -e.deltaY * scale,
    modifiers: modifiersOf(e),
  };
}

/** A Safari gesture's `scale` (relative to its start) as the change since `previous`. */
export function magnifyOfGesture(scale: number, previous: number): number {
  return scale / previous - 1;
}

export function magnifyEvent(
  e: { clientX: number; clientY: number },
  magnification: number,
  origin: Origin,
): CoreMagnifyEvent {
  return { x: e.clientX - origin.left, y: e.clientY - origin.top, magnification };
}
