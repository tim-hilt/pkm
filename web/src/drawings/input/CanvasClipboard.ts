import type { Editor } from "../../core/Core";
import { browserImageCodec } from "../images/BrowserImageCodec";
import type { ImageCodec } from "../images/ImageImporter";

/** One thing on the clipboard, offered in several types; the Async Clipboard API's `ClipboardItem`. */
export interface ClipboardEntry {
  readonly types: readonly string[];
  getType(type: string): Promise<Blob>;
}

/** What is used of the Async Clipboard API (`navigator.clipboard`). */
export interface SystemClipboard {
  writeText(text: string): Promise<void>;
  read(): Promise<ClipboardEntry[]>;
  write(items: ClipboardItem[]): Promise<void>;
}

/** What the clipboard needs of the Canvas. */
export interface ClipboardHost {
  /** Images pasted, as the bytes of their files, and where (in the Canvas's coordinates); none for the middle of the view. */
  images(sources: Uint8Array[], at?: { x: number; y: number }): void;
  /** The editor was changed by a cut or a paste of Elements. */
  changed(): void;
}

/** What is used of a clipboard event's `clipboardData`. */
export interface ClipboardTransfer {
  setData(type: string, text: string): void;
  getData(type: string): string;
  readonly files: ArrayLike<File>;
}

type Point = { x: number; y: number };

/**
 * Copying, cutting and pasting, as the Swift `CanvasClipboard` does: the text
 * on the clipboard is Excalidraw's clipboard JSON (`excalidraw/clipboard`) as
 * plain text, which the core writes and reads, so Elements move between this
 * app, excalidraw.com and the native app. Images on the clipboard go to the
 * image insertion. What else is there is not ours to paste.
 *
 * Two routes, as on excalidraw.com: the keyboard and the browser's Edit menu
 * arrive as the page's copy, cut and paste events (`listen`), which need no
 * permission; the Context menu has no such event and uses the Async Clipboard
 * API (`copy`, `cut`, `paste`), which the browser may refuse (permission, no
 * focus): then nothing happens.
 */
export class CanvasClipboard {
  constructor(
    private readonly editor: Pick<
      Editor,
      "copySelection" | "cutSelection" | "paste" | "selectedImage" | "fileBytes"
    >,
    private readonly system: SystemClipboard,
    private readonly host: ClipboardHost,
    private readonly codec: ImageCodec = browserImageCodec,
  ) {}

  /**
   * Takes the copy, cut and paste events that reach `page` (the document),
   * except those of text fields, which keep their own; a paste lands around
   * `pastePoint()` (in the Canvas's coordinates: the pointer, when it is over
   * the Canvas) or in the middle of the view. Returns what stops it.
   */
  listen(page: EventTarget, pastePoint: () => Point | undefined): () => void {
    const listeners = new AbortController();
    const on = (type: string, handle: (data: ClipboardTransfer) => boolean) =>
      page.addEventListener(
        type,
        (e) => {
          const data = (e as ClipboardEvent).clipboardData;
          if (data && !isTextField(e.target) && handle(data)) e.preventDefault();
        },
        { signal: listeners.signal },
      );
    on("copy", (data) => this.put(data, this.editor.copySelection()));
    on("cut", (data) => {
      const cut = this.put(data, this.editor.cutSelection());
      if (cut) this.host.changed();
      return cut;
    });
    on("paste", (data) => {
      const at = pastePoint();
      if (this.pasteText(data.getData("text/plain"), at)) return true;
      const images = Array.from(data.files).filter((file) => file.type.startsWith("image/"));
      if (images.length === 0) return false;
      void Promise.all(images.map(async (file) => new Uint8Array(await file.arrayBuffer()))).then(
        (sources) => this.host.images(sources, at),
      );
      return true;
    });
    return () => listeners.abort();
  }

  /** Puts the Selection on the clipboard. */
  async copy(): Promise<void> {
    await this.write(this.editor.copySelection());
  }

  /**
   * Puts the selected image on the clipboard as PNG, the one picture type
   * every browser writes. Its bytes are handed over as a promise, so that
   * Safari still counts the write as part of the click that asked for it.
   */
  async copyImage(): Promise<void> {
    const id = this.editor.selectedImage();
    const bytes = id === undefined ? undefined : this.editor.fileBytes(id);
    if (!bytes) return;
    const png = (async () => {
      const isPng = [0x89, 0x50, 0x4e, 0x47].every((byte, at) => bytes[at] === byte);
      const out = isPng ? bytes : (await this.codec.encode(bytes, Infinity, "image/png", 1))?.bytes;
      if (!out) throw new Error("Image can't be read");
      return new Blob([out as Uint8Array<ArrayBuffer>], { type: "image/png" });
    })();
    try {
      await this.system.write([new ClipboardItem({ "image/png": png })]);
    } catch {
      // Refused or unreadable: nothing happens.
    }
  }

  /** Puts the Selection on the clipboard and deletes it from the Scene. */
  async cut(): Promise<void> {
    const text = this.editor.cutSelection();
    if (text === undefined) return;
    this.host.changed();
    await this.write(text);
  }

  /** Pastes Elements from the clipboard, else images, around `at` (in the Canvas's coordinates) or in the middle of the view. */
  async paste(at?: Point): Promise<void> {
    let entries: ClipboardEntry[];
    try {
      entries = await this.system.read();
    } catch {
      return;
    }
    const images: Uint8Array[] = [];
    for (const entry of entries) {
      if (entry.types.includes("text/plain")) {
        if (this.pasteText(await (await entry.getType("text/plain")).text(), at)) return;
      }
      const type = entry.types.find((t) => t.startsWith("image/"));
      if (type) images.push(new Uint8Array(await (await entry.getType(type)).arrayBuffer()));
    }
    if (images.length > 0) this.host.images(images, at);
  }

  /** Pastes `text` if it is Excalidraw clipboard JSON; whether it was. */
  private pasteText(text: string, at: Point | undefined): boolean {
    if (!this.editor.paste(text, at)) return false;
    this.host.changed();
    return true;
  }

  /** Puts `text` into a clipboard event's data; whether there was any. */
  private put(data: ClipboardTransfer, text: string | undefined): boolean {
    if (text === undefined) return false;
    data.setData("text/plain", text);
    return true;
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

/** Whether `target` is a text field (the Text view, a name, the hex field), which keeps its own copy, cut and paste. */
function isTextField(target: EventTarget | null): boolean {
  const element = target as Partial<HTMLElement> | null;
  return element?.isContentEditable === true || /^(input|textarea)$/i.test(element?.tagName ?? "");
}
