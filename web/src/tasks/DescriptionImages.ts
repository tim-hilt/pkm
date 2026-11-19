import { browserImageCodec } from "../drawings/images/BrowserImageCodec";
import type { ImageCodec } from "../drawings/images/ImageImporter";

/** Where a Description's Images are kept (the Tasks store, ADR-0029). */
export interface ImageFiles {
  /** Keeps a PNG, JPEG, GIF or WebP as it is; its reference. `undefined` for any other format. */
  addImage(bytes: Uint8Array): string | undefined;
  /** The bytes of the Image a reference names; `undefined` if there is none. */
  readImage(reference: string): Promise<Uint8Array | undefined>;
}

/**
 * The Images of a Description: adds pictures, converting what isn't a web
 * format to PNG through a canvas, and shows them as `<img>` from blob URLs,
 * made once per reference until `dispose`. It remembers what it added, for
 * the view around it to release (`takeAdded`).
 */
export class DescriptionImages {
  private readonly urls = new Map<string, Promise<string | undefined>>();
  /** The URLs already made, to show an Image on the draw that asks for it. */
  private readonly ready = new Map<string, string>();
  private added: string[] = [];

  constructor(
    private readonly files: ImageFiles,
    private readonly codec: ImageCodec = browserImageCodec,
    /** The system clipboard unless handed in. */
    private readonly clipboard?: Pick<Clipboard, "write">,
  ) {}

  /** Stores a picture; its reference, or `undefined` if it is no picture the browser reads. */
  async add(bytes: Uint8Array): Promise<string | undefined> {
    let reference = this.files.addImage(bytes);
    if (reference === undefined) {
      const png = await this.codec.encode(bytes, Infinity, "image/png", 1);
      reference = png && this.files.addImage(png.bytes);
    }
    if (reference !== undefined) this.added.push(reference);
    return reference;
  }

  /**
   * Puts the Image `reference` on the clipboard as PNG, the one picture type
   * every browser writes. Its bytes are handed over as a promise, so that
   * Safari still counts the write as part of the click that asked for it.
   */
  copy(reference: string): Promise<void> {
    const png = this.files.readImage(reference).then(async (bytes) => {
      const png =
        bytes &&
        (isPng(bytes) ? bytes : (await this.codec.encode(bytes, Infinity, "image/png", 1))?.bytes);
      if (!png) throw new Error(`Image ${reference} can't be read`);
      return new Blob([png as Uint8Array<ArrayBuffer>], { type: "image/png" });
    });
    return (this.clipboard ?? navigator.clipboard).write([new ClipboardItem({ "image/png": png })]);
  }

  /** The references `add` returned since the last call. */
  takeAdded(): string[] {
    const added = this.added;
    this.added = [];
    return added;
  }

  /** Makes `image` show the Image `reference`, as soon as it is read. */
  show(image: HTMLImageElement, reference: string): void {
    const url = this.ready.get(reference);
    if (url !== undefined) {
      image.src = url;
      return;
    }
    let pending = this.urls.get(reference);
    if (!pending) {
      pending = this.files.readImage(reference).then((bytes) => {
        if (!bytes) return undefined;
        const url = URL.createObjectURL(new Blob([bytes as Uint8Array<ArrayBuffer>]));
        this.ready.set(reference, url);
        return url;
      });
      this.urls.set(reference, pending);
    }
    void pending.then((url) => {
      if (url !== undefined) image.src = url;
    });
  }

  /** Lets the browser free the pictures, for when the Description goes away. */
  dispose(): void {
    for (const url of this.ready.values()) URL.revokeObjectURL(url);
    this.ready.clear();
    this.urls.clear();
  }
}

const isPng = (bytes: Uint8Array) =>
  [0x89, 0x50, 0x4e, 0x47].every((byte, at) => bytes[at] === byte);
