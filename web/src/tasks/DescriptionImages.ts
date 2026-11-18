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
 * The Images of Descriptions: adds pictures, converting what isn't a web
 * format to PNG through a canvas, and shows them as `<img>` from blob URLs,
 * made once per reference until `dispose`.
 */
export class DescriptionImages {
  private readonly urls = new Map<string, Promise<string | undefined>>();
  /** The URLs already made, to show an Image on the draw that asks for it. */
  private readonly ready = new Map<string, string>();

  constructor(
    private readonly files: ImageFiles,
    private readonly codec: ImageCodec = browserImageCodec,
  ) {}

  /** Stores a picture; its reference, or `undefined` if it is no picture the browser reads. */
  async add(bytes: Uint8Array): Promise<string | undefined> {
    const kept = this.files.addImage(bytes);
    if (kept !== undefined) return kept;
    const png = await this.codec.encode(bytes, Infinity, "image/png", 1);
    return png && this.files.addImage(png.bytes);
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
