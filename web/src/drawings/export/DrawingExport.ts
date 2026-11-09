import type { ExportScope } from "../../core/Core";
import type { OpenDrawing } from "../gallery/OpenDrawing";

/** A file for the user to keep. */
export interface DownloadedFile {
  name: string;
  bytes: Uint8Array;
  mimeType: string;
}

/** What `DrawingExport` needs from its surroundings: the browser's way to hand a file to the user, and the PNG of the Drawing. */
export interface DrawingExportPorts {
  save(file: DownloadedFile): void;
  /** `undefined` if there is nothing to export. */
  png(scope: ExportScope, background: boolean): Promise<Uint8Array | undefined>;
}

/**
 * What the main menu does with the open Drawing: downloads it as the
 * `.excalidraw` file exactly as stored, or as PNG; both named after the
 * Drawing. A port of the Swift `DrawingExporter`.
 */
export class DrawingExport {
  constructor(
    private readonly drawing: OpenDrawing,
    private readonly ports: DrawingExportPorts,
  ) {}

  /** What the Drawing is saved as, with every change so far in it. */
  async downloadDrawing(): Promise<void> {
    this.ports.save({
      name: `${this.drawing.name}.excalidraw`,
      bytes: await this.drawing.file(),
      mimeType: "application/vnd.excalidraw+json",
    });
  }

  /** Does nothing if there is nothing to export (an empty Scene, no Selection). */
  async downloadPng(scope: ExportScope, background: boolean): Promise<void> {
    const bytes = await this.ports.png(scope, background);
    if (bytes) this.ports.save({ name: `${this.drawing.name}.png`, bytes, mimeType: "image/png" });
  }
}

/** Hands `file` to the user through the browser's download. */
export function downloadInBrowser({ name, bytes, mimeType }: DownloadedFile): void {
  const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>], { type: mimeType }));
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.click();
  // The download has started by the time the task is over.
  setTimeout(() => URL.revokeObjectURL(url));
}
