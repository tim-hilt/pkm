/** The editor's zoom commands, all the zoom controls need. */
export interface Zoom {
  zoomIn(): void;
  zoomOut(): void;
  resetZoom(): void;
  zoomToFit(): void;
  viewport(): { zoom: number };
}

/** The zoom in whole percent, as the reset button shows it; whole percent, so a pinch updates the label only when the number does. */
export const zoomPercent = (zoom: number): number => Math.round(zoom * 100);

/**
 * Excalidraw's zoom group, bottom left: out, reset (the percentage), in; and
 * zoom to fit, until the main menu and shortcuts have it. The core is the
 * source of truth: `update` re-reads the zoom after every event.
 */
export class ZoomControls {
  readonly element = document.createElement("div");
  private readonly percent: HTMLButtonElement;

  constructor(
    private readonly editor: Zoom,
    private readonly onChange: () => void,
  ) {
    this.element.className = "zoom-controls";
    this.element.setAttribute("role", "group");
    this.element.setAttribute("aria-label", "Zoom");
    this.percent = this.button("Reset zoom", "100%", () => editor.resetZoom());
    this.percent.classList.add("zoom-percent");
    this.element.replaceChildren(
      this.button("Zoom out", "−", () => editor.zoomOut()),
      this.percent,
      this.button("Zoom in", "+", () => editor.zoomIn()),
      this.button("Zoom to fit", "⤢", () => editor.zoomToFit()),
    );
    this.update();
  }

  /** Shows the editor's zoom. */
  update(): void {
    const text = `${zoomPercent(this.editor.viewport().zoom)}%`;
    if (this.percent.textContent !== text) {
      this.percent.textContent = text;
      this.percent.setAttribute("aria-valuetext", text);
    }
  }

  private button(label: string, text: string, run: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = text;
    button.setAttribute("aria-label", label);
    button.title = label;
    button.addEventListener("click", () => {
      run();
      this.onChange();
    });
    return button;
  }
}
