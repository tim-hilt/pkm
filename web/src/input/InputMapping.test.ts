import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { Cursor, PointerButton, PointerKind, PointerPhase, Tool, loadCore } from "../core/Core";
import type { Core } from "../core/Core";
import {
  type PointerData,
  type WheelData,
  lostCapturePhase,
  magnifyOfGesture,
  pointerButton,
  pointerEvent,
  pointerKind,
  scrollEvent,
} from "./InputMapping";

const none = { shiftKey: false, ctrlKey: false, altKey: false, metaKey: false };
const origin = { left: 100, top: 50 };

const pointer = (over: Partial<PointerData>): PointerData => ({
  pointerId: 1,
  pointerType: "mouse",
  clientX: 110,
  clientY: 70,
  pressure: 0.5,
  timeStamp: 2500,
  ...none,
  ...over,
});

const wheel = (over: Partial<WheelData>): WheelData => ({
  clientX: 110,
  clientY: 70,
  deltaX: 0,
  deltaY: 0,
  deltaMode: 0,
  ...none,
  ...over,
});

// What the Swift client sends for the same input, written out by hand from
// CanvasView.swift: `sendMouse`, the touch `send`, `scrollWheel` and `magnify`.
describe("pointer events", () => {
  it("maps pointerType to the core's pointer kind as Swift maps UITouch.TouchType", () => {
    expect(pointerKind("touch")).toBe(PointerKind.Finger);
    expect(pointerKind("pen")).toBe(PointerKind.Pencil);
    expect(pointerKind("mouse")).toBe(PointerKind.Mouse);
  });

  it("maps buttons: right is secondary, wheel is middle, the rest primary", () => {
    expect(pointerButton(0)).toBe(PointerButton.Primary);
    expect(pointerButton(1)).toBe(PointerButton.Middle);
    expect(pointerButton(2)).toBe(PointerButton.Secondary);
    expect(pointerButton(3)).toBe(PointerButton.Primary);
    expect(pointerButton(4)).toBe(PointerButton.Primary);
  });

  it("forwards position relative to the Canvas, seconds and 0.5 pressure for a mouse", () => {
    expect(
      pointerEvent(pointer({ pressure: 0 }), PointerPhase.Down, PointerButton.Primary, origin),
    ).toEqual({
      id: 1,
      phase: PointerPhase.Down,
      x: 10,
      y: 20,
      kind: PointerKind.Mouse,
      button: PointerButton.Primary,
      pressure: 0.5,
      timestamp: 2.5,
      modifiers: { shift: false, ctrl: false, alt: false, meta: false },
    });
  });

  it("forwards a pen's pressure but not a finger's", () => {
    const pen = pointerEvent(
      pointer({ pointerType: "pen", pressure: 0.8 }),
      PointerPhase.Move,
      PointerButton.Primary,
      origin,
    );
    expect(pen.kind).toBe(PointerKind.Pencil);
    expect(pen.pressure).toBeCloseTo(0.8);
    const finger = pointerEvent(
      pointer({ pointerType: "touch", pressure: 1 }),
      PointerPhase.Move,
      PointerButton.Primary,
      origin,
    );
    expect(finger.pressure).toBe(0.5);
  });

  it("forwards the modifier keys", () => {
    const event = pointerEvent(
      pointer({ shiftKey: true, metaKey: true }),
      PointerPhase.Up,
      PointerButton.Primary,
      origin,
    );
    expect(event.modifiers).toEqual({ shift: true, ctrl: false, alt: false, meta: true });
  });
});

describe("lost pointer capture", () => {
  // A quick release can lose the capture before its pointerup arrives: a
  // fast drag would be dropped instead of drawn.
  it("is the release when no button is down any more", () => {
    expect(lostCapturePhase(0)).toBe(PointerPhase.Up);
  });

  it("cancels while a button is still down", () => {
    expect(lostCapturePhase(1)).toBe(PointerPhase.Cancel);
  });
});

describe("wheel events", () => {
  it("moves the content opposite to the wheel delta, in points", () => {
    expect(scrollEvent(wheel({ deltaX: 3, deltaY: 40 }), origin, 600)).toEqual({
      x: 10,
      y: 20,
      dx: -3,
      dy: -40,
      modifiers: { shift: false, ctrl: false, alt: false, meta: false },
    });
  });

  it("converts lines to 10 points, as Swift does, and pages to the view height", () => {
    const lines = scrollEvent(wheel({ deltaY: 3, deltaMode: 1 }), origin, 600);
    expect(lines.dy).toBe(-30);
    const pages = scrollEvent(wheel({ deltaY: 1, deltaMode: 2 }), origin, 600);
    expect(pages.dy).toBe(-600);
  });

  it("keeps ctrl, which is how browsers report a trackpad pinch", () => {
    expect(scrollEvent(wheel({ deltaY: -4, ctrlKey: true }), origin, 600).modifiers.ctrl).toBe(
      true,
    );
  });

  it("turns a Safari gesture's scale into the change since the last one", () => {
    expect(magnifyOfGesture(1.1, 1)).toBeCloseTo(0.1);
    expect(magnifyOfGesture(1.21, 1.1)).toBeCloseTo(0.1);
  });
});

// The same events through the real core.
describe("input through the core", () => {
  let core: Core;
  beforeAll(async () => {
    core = await loadCore(
      readFileSync(new URL("../../generated/pkm_web_bg.wasm", import.meta.url)),
    );
  });

  const editor = () => {
    const e = core.makeEditor();
    e.setViewSize(800, 600);
    return e;
  };

  it("pans with the Hand Tool and shows the grabbing cursor while dragging", () => {
    const e = editor();
    e.setTool(Tool.Hand);
    expect(e.cursor(50, 50)).toBe(Cursor.Grab);
    const send = (phase: PointerPhase, x: number) =>
      e.pointer(pointerEvent(pointer({ clientX: x + 100, clientY: 50 }), phase, 0, origin));
    send(PointerPhase.Down, 100);
    send(PointerPhase.Move, 60);
    expect(e.cursor(60, 0)).toBe(Cursor.Grabbing);
    send(PointerPhase.Up, 60);
    expect(e.viewport().originX).toBeCloseTo(40);
    expect(e.cursor(60, 0)).toBe(Cursor.Grab);
    e.free();
  });

  it("scrolls with the wheel and zooms with ctrl+wheel around the pointer", () => {
    const e = editor();
    e.scroll(scrollEvent(wheel({ deltaY: 30 }), origin, 600));
    expect(e.viewport().originY).toBeCloseTo(30);
    e.scroll(scrollEvent(wheel({ deltaY: -10, ctrlKey: true }), origin, 600));
    expect(e.viewport().zoom).toBeGreaterThan(1);
    e.free();
  });

  it("pans with two fingers and pinches", () => {
    const e = editor();
    const finger = (id: number, phase: PointerPhase, x: number, y: number) =>
      e.pointer(
        pointerEvent(
          pointer({ pointerId: id, pointerType: "touch", clientX: x + 100, clientY: y + 50 }),
          phase,
          0,
          origin,
        ),
      );
    finger(1, PointerPhase.Down, 100, 100);
    finger(2, PointerPhase.Down, 200, 100);
    finger(1, PointerPhase.Move, 50, 100);
    finger(2, PointerPhase.Move, 250, 100);
    expect(e.viewport().zoom).toBeCloseTo(2);
    e.free();
  });

  it("changes the revision for a drawing but not for navigation", () => {
    const e = editor();
    const before = e.revision();
    e.zoomIn();
    e.scroll(scrollEvent(wheel({ deltaY: 30 }), origin, 600));
    expect(e.revision()).toBe(before);
    e.setTool(Tool.Rectangle);
    for (const [phase, x] of [
      [PointerPhase.Down, 10],
      [PointerPhase.Move, 90],
      [PointerPhase.Up, 90],
    ] as const) {
      e.pointer(pointerEvent(pointer({ clientX: x + 100, clientY: x + 50 }), phase, 0, origin));
    }
    expect(e.revision()).not.toBe(before);
    expect(e.elementCount()).toBe(1);
    e.free();
  });
});
