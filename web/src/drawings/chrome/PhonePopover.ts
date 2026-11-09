import { type Rect, popoverPlacement } from "./DrawingLayout";

/**
 * The phone's own popover: excalidraw.com's island with a pointer tip toward
 * its anchor. It sits per `popoverPlacement`, scrolls beyond its maximum
 * height, and for screen readers is a dialog that Escape closes. A port of the
 * Swift `PhonePopover`.
 *
 * Between the Canvas and the chrome while it is open there is a scrim that
 * takes every touch (tap, drag, pinch) so that it only closes the popover and
 * never reaches the Canvas (ADR-0013). It outlives the popover by the touch
 * that closed it, so that the rest of that gesture, a pinch's second finger,
 * is not handed to the Canvas either.
 */
export class PhonePopover {
  readonly element = document.createElement("div");
  readonly scrim = document.createElement("div");
  private readonly tip = document.createElement("div");
  private readonly content = document.createElement("div");
  private readonly touches = new Set<number>();
  private shown: { anchor: HTMLElement; bar: HTMLElement } | undefined;

  constructor(
    /** The container whose coordinates the placement uses. */
    private readonly container: HTMLElement,
    private readonly onClose: () => void,
  ) {
    this.scrim.className = "popover-scrim";
    this.scrim.hidden = true;
    const release = (e: PointerEvent) => {
      this.touches.delete(e.pointerId);
      this.updateScrim();
    };
    this.scrim.addEventListener("pointerdown", (e) => {
      this.touches.add(e.pointerId);
      this.scrim.setPointerCapture(e.pointerId);
      if (this.shown) this.onClose();
    });
    this.scrim.addEventListener("pointerup", release);
    this.scrim.addEventListener("pointercancel", release);

    this.element.className = "phone-popover";
    this.element.setAttribute("role", "dialog");
    this.element.hidden = true;
    this.element.addEventListener("keydown", (e) => {
      if (e.key === "Escape") this.onClose();
    });
    this.content.className = "popover-content";
    this.tip.className = "popover-tip";
    this.tip.setAttribute("aria-hidden", "true");
    this.element.append(this.content, this.tip);
  }

  get isOpen(): boolean {
    return this.shown !== undefined;
  }

  /** Shows `content` over `bar`, pointing at `anchor`. */
  show(label: string, content: Node[], anchor: HTMLElement, bar: HTMLElement): void {
    this.shown = { anchor, bar };
    this.element.setAttribute("aria-label", label);
    this.content.replaceChildren(...content);
    this.element.hidden = false;
    this.scrim.hidden = false;
    this.place();
  }

  hide(): void {
    if (!this.shown) return;
    this.shown = undefined;
    this.element.hidden = true;
    this.updateScrim();
  }

  /** Places the open popover again: its content or the layout may have changed. */
  place(): void {
    if (!this.shown) return;
    const origin = this.container.getBoundingClientRect();
    const rect = (element: HTMLElement): Rect => {
      const r = element.getBoundingClientRect();
      return { x: r.left - origin.left, y: r.top - origin.top, width: r.width, height: r.height };
    };
    const style = this.element.style;
    style.width = style.height = "";
    style.maxWidth = "none";
    const natural = this.element.getBoundingClientRect();
    const { frame, pointerX } = popoverPlacement(
      rect(this.shown.anchor),
      natural,
      origin,
      rect(this.shown.bar).y,
    );
    style.maxWidth = "";
    style.left = `${frame.x}px`;
    style.top = `${frame.y}px`;
    style.width = `${frame.width}px`;
    style.height = `${frame.height}px`;
    this.tip.style.left = `${pointerX - frame.x}px`;
  }

  private updateScrim(): void {
    this.scrim.hidden = !this.shown && this.touches.size === 0;
  }
}
