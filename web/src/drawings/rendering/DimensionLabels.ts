import type { Dimensions, Viewport } from "../../core/Core";

/**
 * The numbers of Dimensions over the Canvas, a port of the Swift
 * `DimensionLabels`: the system font at a constant screen size, each centred
 * on the Scene point the core gives and turned by its angle. The lines and
 * ticks are the renderer's (`setDimensions`).
 */
export class DimensionLabels {
  /** Holds one element per label; add it over the Canvas. */
  readonly element = document.createElement("div");

  constructor() {
    this.element.className = "dimension-labels";
    this.element.setAttribute("aria-hidden", "true");
  }

  /** Shows `dimensions`' numbers in `viewport` at `strength` (1 full, down to 0); undefined takes them away. */
  show(dimensions: Dimensions | undefined, viewport: Viewport, strength: number): void {
    const labels = dimensions?.labels ?? [];
    // Only the number of elements follows the labels, so that nothing flashes.
    while (this.element.children.length > labels.length) this.element.lastElementChild!.remove();
    while (this.element.children.length < labels.length) {
      this.element.append(document.createElement("span"));
    }
    this.element.style.opacity = String(strength);
    if (!dimensions) return;
    const color = cssColor(dimensions.color, dimensions.opacity);
    labels.forEach((label, index) => {
      const span = this.element.children[index] as HTMLElement;
      span.textContent = label.text;
      span.style.color = color;
      const x = (label.x - viewport.originX) * viewport.zoom;
      const y = (label.y - viewport.originY) * viewport.zoom;
      span.style.transform = `translate(${x}px, ${y}px) translate(-50%, -50%) rotate(${label.angle}rad)`;
    });
  }
}

/** `rgba` (`0xRRGGBBAA`) at `opacity`, as CSS. */
function cssColor(rgba: number, opacity: number): string {
  const channel = (shift: number) => (rgba >>> shift) & 0xff;
  return `rgb(${channel(24)} ${channel(16)} ${channel(8)} / ${(channel(0) / 255) * opacity})`;
}
