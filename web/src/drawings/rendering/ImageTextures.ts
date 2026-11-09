import type { TextureLoader } from "./TextureUpload";

/** Where the renderer gets the bytes of image files from: the core's `Editor`. */
export interface ImageFileSource {
  /** The stored bytes of file `id`, or `undefined` if there is no such file. */
  fileBytes(id: string): Uint8Array | undefined;
}

type Entry =
  | { state: "loading" }
  | { state: "texture"; texture: GPUTexture }
  /** The Drawing has no bytes for the file. */
  | { state: "missing" }
  /** The file is there but is not an image the browser can read. */
  | { state: "undecodable" };

/**
 * Textures of image files, one per file id however many Elements show it. A
 * port of the Swift `ImageTextures`, except that the browser decodes
 * asynchronously: the first time a file is asked for, its load starts and
 * there is no texture yet (the renderer shows a placeholder); `onChange` tells
 * when there is one, or when the file turned out to be no image.
 *
 * A file that is missing or cannot be decoded is remembered as such, so that
 * asking again does not repeat the work. `keepOnly` releases what nothing
 * refers to any more.
 */
export class ImageTextures {
  /** Called with the id of a file whose texture arrived or which cannot be decoded. */
  onChange: ((id: string) => void) | undefined;

  private entries = new Map<string, Entry>();
  private readonly pending = new Set<Promise<void>>();
  /** Files decoded so far, successfully or not; tests use it to see what is cached. Missing files are not counted. */
  decodeCount = 0;

  constructor(private readonly load: TextureLoader) {}

  /** Number of textures held. */
  get count(): number {
    let count = 0;
    for (const entry of this.entries.values()) if (entry.state === "texture") count++;
    return count;
  }

  /**
   * The texture of file `id`; `undefined` while it is not there: loading, or
   * because the file is missing or is not an image.
   */
  texture(id: string, files: ImageFileSource | undefined): GPUTexture | undefined {
    const entry = this.entries.get(id);
    if (entry) return entry.state === "texture" ? entry.texture : undefined;
    const loading: Entry = { state: "loading" };
    this.entries.set(id, loading);
    const work = this.read(id, files, loading).finally(() => this.pending.delete(work));
    this.pending.add(work);
    return undefined;
  }

  /** Whether the file is there but could not be decoded, as of the last time it was asked for. */
  isUndecodable(id: string): boolean {
    return this.entries.get(id)?.state === "undecodable";
  }

  /** Resolves once every load started so far is over. */
  async settled(): Promise<void> {
    while (this.pending.size > 0) await Promise.all(this.pending);
  }

  /** Releases the textures of all files not in `referenced`. */
  keepOnly(referenced: ReadonlySet<string>): void {
    for (const [id, entry] of this.entries) {
      if (referenced.has(id)) continue;
      if (entry.state === "texture") entry.texture.destroy();
      this.entries.delete(id);
    }
  }

  /** Forgets which files were unusable. A Drawing that was loaded since may have them. */
  forgetFailures(): void {
    for (const [id, entry] of this.entries) {
      if (entry.state === "missing" || entry.state === "undecodable") this.entries.delete(id);
    }
  }

  private async read(id: string, files: ImageFileSource | undefined, entry: Entry): Promise<void> {
    // The bytes are read only after the caller is done: it may be looking at
    // the core's buffers, which reading a file (it allocates) could move.
    await Promise.resolve();
    if (this.entries.get(id) !== entry) return;
    const bytes = files?.fileBytes(id);
    if (!bytes) {
      this.entries.set(id, { state: "missing" });
      return;
    }
    this.decodeCount++;
    const texture = await this.load(bytes).catch(() => undefined);
    if (this.entries.get(id) !== entry) {
      // Released while it loaded.
      texture?.destroy();
      return;
    }
    this.entries.set(id, texture ? { state: "texture", texture } : { state: "undecodable" });
    this.onChange?.(id);
  }
}
