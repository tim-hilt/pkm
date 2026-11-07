import {
  type CorePointerEvent,
  type CoreScrollEvent,
  type CoreMagnifyEvent,
  PointerButton,
  PointerPhase,
} from "../core/Core";
import {
  magnifyEvent,
  magnifyOfGesture,
  pointerButton,
  pointerEvent,
  scrollEvent,
} from "./InputMapping";

/** Where the Canvas sends what the user does to it. */
export interface InputSink {
  pointer(event: CorePointerEvent): void;
  scroll(event: CoreScrollEvent): void;
  magnify(event: CoreMagnifyEvent): void;
  setPanKey(held: boolean): void;
  /** Enter was pressed with nothing else taking it: edits the text of the Selection. */
  editSelectedText(): void;
  /** The mouse moved over the Canvas without a button down. */
  hover(x: number, y: number): void;
}

// Safari's gesture events, which the DOM types lack.
interface GestureEvent extends UIEvent {
  scale: number;
  clientX: number;
  clientY: number;
}

/**
 * Forwards the Pointer Events, wheel and space bar on a canvas to the core
 * (ADR-0006) as the Swift client does, and keeps the browser out of it: no
 * page zoom, text-selection callout, magnifier, browser context menu or
 * overscroll (the rest is CSS: `touch-action: none`, see `index.html`).
 */
export class CanvasInput {
  /** The button each pressed pointer went down with: moves and ups do not say. */
  private readonly pressed = new Map<number, PointerButton>();
  private lastGestureScale = 1;
  private touches = 0;
  private readonly listeners = new AbortController();

  constructor(
    private readonly canvas: HTMLCanvasElement,
    private readonly sink: InputSink,
  ) {
    const { signal } = this.listeners;
    const on = <K extends keyof HTMLElementEventMap>(
      type: K,
      listener: (event: HTMLElementEventMap[K]) => void,
    ) => canvas.addEventListener(type, listener, { signal });
    on("pointerdown", (e) => this.down(e));
    on("pointermove", (e) => this.move(e));
    on("pointerup", (e) => this.up(e, PointerPhase.Up));
    on("pointercancel", (e) => this.up(e, PointerPhase.Cancel));
    on("lostpointercapture", (e) => this.up(e, PointerPhase.Cancel));
    // Passive listeners cannot stop the page from zooming or scrolling.
    canvas.addEventListener("wheel", (e) => this.wheel(e), { passive: false, signal });
    on("contextmenu", (e) => e.preventDefault());
    on("dragstart", (e) => e.preventDefault());
    on("selectstart", (e) => e.preventDefault());
    // The browser would move the focus from the Text view to the page when the
    // Canvas is pressed; the core ends the editing itself (a tap commits).
    on("mousedown", (e) => e.preventDefault());
    for (const type of ["gesturestart", "gesturechange", "gestureend"]) {
      canvas.addEventListener(type, (e) => this.gesture(type, e as GestureEvent), {
        passive: false,
        signal,
      });
    }
    window.addEventListener("keydown", (e) => this.key(e, true), { signal });
    window.addEventListener("keyup", (e) => this.key(e, false), { signal });
    window.addEventListener("blur", () => sink.setPanKey(false), { signal });
  }

  /** Stops listening, for when the Canvas goes away. */
  dispose(): void {
    this.listeners.abort();
  }

  private origin() {
    return this.canvas.getBoundingClientRect();
  }

  private down(e: PointerEvent): void {
    // A pointer is down once: a second mouse button joins its press.
    if (this.pressed.has(e.pointerId)) return;
    // Keeps the browser from selecting, dragging or focusing; but the chrome
    // loses focus as on any press elsewhere, or a toolbar button clicked last
    // would keep the space bar from panning.
    e.preventDefault();
    if (document.activeElement instanceof HTMLElement) document.activeElement.blur();
    if (e.pointerType === "touch") this.touches++;
    const button = pointerButton(e.button);
    this.pressed.set(e.pointerId, button);
    // Moves and the end reach the Canvas even outside of it.
    this.canvas.setPointerCapture(e.pointerId);
    this.sink.pointer(pointerEvent(e, PointerPhase.Down, button, this.origin()));
  }

  private move(e: PointerEvent): void {
    const origin = this.origin();
    const button = this.pressed.get(e.pointerId);
    if (button === undefined) {
      // Hovering: only the mouse's cursor cares, as on the Mac.
      if (e.pointerType === "mouse")
        this.sink.hover(e.clientX - origin.left, e.clientY - origin.top);
      return;
    }
    // A moving pointer is forwarded with every sample the browser coalesced
    // into it (a pen samples faster than the display refreshes).
    const samples = e.getCoalescedEvents?.() ?? [];
    for (const sample of samples.length > 0 ? samples : [e]) {
      this.sink.pointer(pointerEvent(sample, PointerPhase.Move, button, origin));
    }
  }

  private up(e: PointerEvent, phase: PointerPhase): void {
    const button = this.pressed.get(e.pointerId);
    // The capture is also lost after a pointerup, which has been handled.
    if (button === undefined) return;
    this.pressed.delete(e.pointerId);
    if (e.pointerType === "touch") this.touches--;
    this.sink.pointer(pointerEvent(e, phase, button, this.origin()));
  }

  private wheel(e: WheelEvent): void {
    e.preventDefault();
    const origin = this.origin();
    this.sink.scroll(scrollEvent(e, origin, this.canvas.clientHeight));
  }

  /**
   * Safari reports a trackpad pinch as gesture events, not as wheel events,
   * and would zoom the page. Fingers on a touch screen are pinched by the core
   * from the pointers.
   */
  private gesture(type: string, e: GestureEvent): void {
    e.preventDefault();
    if (type === "gesturestart") this.lastGestureScale = 1;
    if (type !== "gesturechange" || this.touches > 0) return;
    this.sink.magnify(
      magnifyEvent(e, magnifyOfGesture(e.scale, this.lastGestureScale), this.origin()),
    );
    this.lastGestureScale = e.scale;
  }

  /** The space bar pans while held, Enter edits the text of the Selection. A text field keeps its keys, a button its Enter. */
  private key(e: KeyboardEvent, down: boolean): void {
    const isSpace = e.code === "Space";
    const isEnter = e.key === "Enter" && !e.metaKey && !e.ctrlKey && !e.altKey && !e.shiftKey;
    if (!isSpace && !isEnter) return;
    const target = e.target;
    const typing =
      target instanceof HTMLElement &&
      (target.isContentEditable || /^(input|textarea|select|button)$/i.test(target.tagName));
    if (isEnter) {
      if (down && !typing) {
        e.preventDefault();
        this.sink.editSelectedText();
      }
      return;
    }
    if (down && typing) return;
    if (down) e.preventDefault();
    this.sink.setPanKey(down);
  }
}
