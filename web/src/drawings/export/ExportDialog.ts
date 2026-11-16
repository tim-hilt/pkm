import { button, make } from "../../Dom";
import type { DrawingExport } from "./DrawingExport";

/**
 * Export, as excalidraw.com's Export image dialog: a preview of the PNG of the
 * Selection, or of the Scene with nothing selected, to download or copy to
 * the clipboard. A port of the Swift `ExportSheet`.
 */
export class ExportDialog {
  readonly element = make("dialog", "export-dialog");
  private readonly preview = make("img", "export-preview");
  private readonly copyButton: HTMLButtonElement;
  private png: Uint8Array | undefined;
  private url: string | undefined;
  private opening = false;

  constructor(private readonly exporter: DrawingExport) {
    const heading = make("h2", "", ["Export image"]);
    heading.id = "export-dialog-title";
    this.element.setAttribute("aria-labelledby", heading.id);
    this.preview.alt = "Preview";
    const close = button("export-close", "✕", () => this.element.close());
    close.setAttribute("aria-label", "Close");
    const download = button("primary", "Download PNG", () => {
      if (this.png) this.exporter.downloadPng(this.png);
    });
    // What `showModal` focuses.
    download.autofocus = true;
    this.copyButton = button("", "Copy to Clipboard", () => void this.copy());
    this.element.append(
      make("div", "export-header", [heading, close]),
      make("div", "export-frame", [this.preview]),
      make("div", "export-buttons", [download, this.copyButton]),
    );
    // The Canvas's shortcuts stay off while it is open; Escape still closes it.
    this.element.addEventListener("keydown", (e) => e.stopPropagation());
    // A press on the backdrop closes it.
    this.element.addEventListener("click", (e) => {
      if (e.target === this.element) this.element.close();
    });
    this.element.addEventListener("close", () => this.clear());
  }

  /** Opens it with the PNG as it is now; not if there is nothing to export. */
  async open(): Promise<void> {
    if (this.element.open || this.opening) return;
    this.opening = true;
    try {
      const png = await this.exporter.image();
      // The Drawing may have been left while the PNG was made.
      if (!png || !this.element.isConnected) return;
      this.png = png;
      this.url = URL.createObjectURL(
        new Blob([png as Uint8Array<ArrayBuffer>], { type: "image/png" }),
      );
      this.preview.src = this.url;
      this.copyButton.textContent = "Copy to Clipboard";
      this.element.showModal();
    } finally {
      this.opening = false;
    }
  }

  private async copy(): Promise<void> {
    if (this.png && (await this.exporter.copyPng(this.png))) {
      this.copyButton.textContent = "Copied";
    }
  }

  private clear(): void {
    if (this.url) URL.revokeObjectURL(this.url);
    this.url = undefined;
    this.png = undefined;
    this.preview.removeAttribute("src");
  }
}
