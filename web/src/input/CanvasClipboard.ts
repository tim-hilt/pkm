import type { Editor } from "../core/Core";

/** One thing on the clipboard, offered in several types; the Async Clipboard API's `ClipboardItem`. */
export interface ClipboardEntry {
  readonly types: readonly string[];
  getType(type: string): Promise<Blob>;
}

/** What is used of the Async Clipboard API (`navigator.clipboard`). */
export interface SystemClipboard {
  writeText(text: string): Promise<void>;
  read(): Promise<ClipboardEntry[]>;
}

/** What the clipboard needs of the Canvas. */
export interface ClipboardHost {
  /** Images pasted, as the bytes of their files, and where (in the Canvas's coordinates); none for the middle of the view. */
  images(sources: Uint8Array[], at?: { x: number; y: number }): void;
  /** The editor was changed by a cut or a paste of Elements. */
  changed(): void;
}

/**
 * Copying, cutting and pasting with the Async Clipboard API, as the Swift
 * `CanvasClipboard` does: the text on the clipboard is Excalidraw's clipboard
 * JSON (`excalidraw/clipboard`) as plain text, which the core writes and
 * reads, so Elements move between this app, excalidraw.com and the native app.
 * Images on the clipboard go to the image insertion. What else is there is not
 * ours to paste. The browser may refuse (permission, no focus): then nothing
 * happens.
 */
export class CanvasClipboard {
  constructor(
    private readonly editor: Pick<Editor, "copySelection" | "cutSelection" | "paste">,
    private readonly system: SystemClipboard,
    private readonly host: ClipboardHost,
  ) {}

  /** Puts the Selection on the clipboard. */
  async copy(): Promise<void> {
    await this.write(this.editor.copySelection());
  }

  /** Puts the Selection on the clipboard and deletes it from the Scene. */
  async cut(): Promise<void> {
    const text = this.editor.cutSelection();
    if (text === undefined) return;
    this.host.changed();
    await this.write(text);
  }

  /** Pastes Elements from the clipboard, else images, around `at` (in the Canvas's coordinates) or in the middle of the view. */
  async paste(at?: { x: number; y: number }): Promise<void> {
    let entries: ClipboardEntry[];
    try {
      entries = await this.system.read();
    } catch {
      return;
    }
    const images: Uint8Array[] = [];
    for (const entry of entries) {
      if (entry.types.includes("text/plain")) {
        const text = await (await entry.getType("text/plain")).text();
        if (this.editor.paste(text, at)) {
          this.host.changed();
          return;
        }
      }
      const type = entry.types.find((t) => t.startsWith("image/"));
      if (type) images.push(new Uint8Array(await (await entry.getType(type)).arrayBuffer()));
    }
    if (images.length > 0) this.host.images(images, at);
  }

  private async write(text: string | undefined): Promise<void> {
    if (text === undefined) return;
    try {
      await this.system.writeText(text);
    } catch {
      // Refused: the Selection stays as it is (after a cut, undo brings it back).
    }
  }
}
