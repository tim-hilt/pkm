import { ExportScope } from "../../core/Core";
import type { OpenDrawing } from "../gallery/OpenDrawing";

/** A file for the user to keep. */
export interface DownloadedFile {
  name: string;
  bytes: Uint8Array;
  mimeType: string;
}

/** What `DrawingExport` needs from its surroundings: the browser's download and clipboard, and the PNG of the Drawing. */
export interface DrawingExportPorts {
  save(file: DownloadedFile): void;
  /** Rejects if the browser refuses. */
  copy(png: Uint8Array): Promise<void>;
  /** `undefined` if there is nothing to export. */
  png(scope: ExportScope): Promise<Uint8Array | undefined>;
}

/**
 * What the main menu does with the open Drawing: Save as, the `.excalidraw`
 * file exactly as stored, and Export, a PNG of the Selection (or of the Scene
 * with nothing selected) to download or copy; files named after the Drawing.
 * A port of the Swift `DrawingFile`.
 */
export class DrawingExport {
  constructor(
    private readonly drawing: OpenDrawing,
    private readonly ports: DrawingExportPorts,
  ) {}

  /** What the Drawing is saved as, with every change so far in it. */
  async saveAs(): Promise<void> {
    this.ports.save({
      name: `${this.drawing.name}.excalidraw`,
      bytes: await this.drawing.file(),
      mimeType: "application/vnd.excalidraw+json",
    });
  }

  /** The PNG Export shows; `undefined` if there is nothing to export. */
  image(): Promise<Uint8Array | undefined> {
    const selected = this.drawing.editor.selection().length > 0;
    return this.ports.png(selected ? ExportScope.Selection : ExportScope.Scene);
  }

  downloadPng(png: Uint8Array): void {
    this.ports.save({ name: `${this.drawing.name}.png`, bytes: png, mimeType: "image/png" });
  }

  /** Whether the browser took it. */
  async copyPng(png: Uint8Array): Promise<boolean> {
    try {
      await this.ports.copy(png);
      return true;
    } catch {
      return false;
    }
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

/** Puts `png` on the clipboard through the Async Clipboard API. */
export function copyInBrowser(png: Uint8Array): Promise<void> {
  const blob = new Blob([png as Uint8Array<ArrayBuffer>], { type: "image/png" });
  return navigator.clipboard.write([new ClipboardItem({ "image/png": blob })]);
}
