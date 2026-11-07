import type { RenderedImage } from "../rendering/RenderedImage";
import type { DrawingStore, Entry } from "./DrawingStore";
import { encodePng } from "./PngEncoder";

/** What the Gallery shows for a Drawing. */
export type Thumbnail =
  /** The picture of the Drawing, a PNG. */
  | { kind: "image"; png: Uint8Array }
  /** The file is no valid Drawing (malformed): the Gallery shows an error. Nothing is ever written to such a file. */
  | { kind: "unreadable" }
  /** No picture for another reason (the Drawing is gone). */
  | { kind: "unavailable" };

/** What draws a thumbnail; `ThumbnailRenderer` does, on the GPU. */
export interface ThumbnailPainter {
  /** The picture of the Drawing in `drawing`'s bytes, or `undefined` if they are no valid Drawing. */
  render(drawing: Uint8Array, dark: boolean): Promise<RenderedImage | undefined>;
}

/** Bytes of pictures kept in memory; the cache of the store answers beyond. */
const memoryLimit = 48 << 20;

/**
 * The thumbnails of the Gallery's Drawings. A Drawing has a thumbnail per
 * appearance (ADR-0010), made when first needed. A thumbnail comes from the
 * first of these that has a current one: memory, the cache of the
 * `DrawingStore` (outside the Drawing's file), and last the renderer, which
 * then also fills the cache. A Drawing that changed has a new
 * `Entry.modified`, so its old thumbnail is simply not current any more.
 * A port of the Swift `Thumbnails`.
 */
export class Thumbnails {
  /** Pictures already made, least recently used first. */
  private readonly memory = new Map<string, Uint8Array>();
  private memoryBytes = 0;
  /** Requests under way, so that asking again while one runs does not draw twice. */
  private readonly pending = new Map<string, Promise<Thumbnail>>();
  /** The Drawings (as name, date and appearance, like the keys of `memory`) found to be no valid Drawing. */
  private readonly unreadable = new Set<string>();
  /** Drawing is one at a time: the GPU step is short but the renderer is one. */
  private turn: Promise<unknown> = Promise.resolve();

  /** Thumbnails drawn so far (not counting those from caches). */
  rendered = 0;

  constructor(
    private readonly store: DrawingStore,
    private readonly painter: ThumbnailPainter,
  ) {}

  /** The thumbnail of the Drawing as listed in `entry`, as the light or the `dark` Canvas shows it. Cheap when called again. */
  thumbnail(entry: Entry, dark = false): Promise<Thumbnail> {
    const key = `${entry.name}\0${entry.modified}\0${dark ? "dark" : "light"}`;
    const png = this.memory.get(key);
    if (png) {
      this.memory.delete(key);
      this.memory.set(key, png);
      return Promise.resolve({ kind: "image", png });
    }
    if (this.unreadable.has(key)) return Promise.resolve({ kind: "unreadable" });
    let pending = this.pending.get(key);
    if (!pending) {
      pending = this.make(entry, dark, key).finally(() => this.pending.delete(key));
      this.pending.set(key, pending);
    }
    return pending;
  }

  private async make(entry: Entry, dark: boolean, key: string): Promise<Thumbnail> {
    let png = await this.store.thumbnail(entry, dark);
    if (!png) {
      let bytes: Uint8Array;
      try {
        bytes = await this.store.open(entry.name);
      } catch {
        return { kind: "unavailable" };
      }
      const image = await this.draw(bytes, dark);
      if (!image) {
        this.unreadable.add(key);
        return { kind: "unreadable" };
      }
      png = await encodePng(image);
      // Failing to cache costs only the drawing of the thumbnail again.
      await this.store.saveThumbnail(png, entry, dark).catch(() => {});
    }
    this.remember(key, png);
    return { kind: "image", png };
  }

  private draw(bytes: Uint8Array, dark: boolean): Promise<RenderedImage | undefined> {
    const drawn = this.turn.then(async () => {
      const image = await this.painter.render(bytes, dark);
      if (image) this.rendered += 1;
      return image;
    });
    this.turn = drawn.catch(() => {});
    return drawn;
  }

  private remember(key: string, png: Uint8Array): void {
    this.memory.set(key, png);
    this.memoryBytes += png.length;
    for (const [oldest, bytes] of this.memory) {
      if (this.memoryBytes <= memoryLimit || oldest === key) break;
      this.memory.delete(oldest);
      this.memoryBytes -= bytes.length;
    }
  }
}
