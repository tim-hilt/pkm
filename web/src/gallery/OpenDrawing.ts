import type { Core, Editor, Viewport } from "../core/Core";
import type { DrawingStore } from "./DrawingStore";

export interface OpenDrawingOptions {
  /** How long after the last change the Drawing is saved; milliseconds. */
  autosaveDelay?: number;
}

const sameViewport = (a: Viewport, b: Viewport) =>
  a.originX === b.originX && a.originY === b.originY && a.zoom === b.zoom;

/**
 * A Drawing opened from the `DrawingStore` into an `Editor`, kept saved: a
 * change is saved `autosaveDelay` after the last one, and `saveNow` saves at
 * once (leaving the Drawing, the page going to the background). A port of the
 * Swift `OpenDrawing`, without following the file outside the app: nothing
 * else changes the Drawings of the web app.
 *
 * It opens at the viewport it was left at, which `saveNow` keeps in the
 * store's cache (not in the file, so panning and zooming never change the
 * Drawing); one that was never left opens at the default viewport.
 *
 * Writes run one after the other, so that a save never overtakes the one
 * before it or the removal of an emptied Drawing.
 */
export class OpenDrawing {
  readonly editor: Editor;
  private readonly delay: number;
  /** The `Editor.revision` of what the file holds. */
  private savedRevision: number;
  /** The viewport the cache holds, or the default if it holds none. */
  private savedViewport: Viewport;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private writes: Promise<void> = Promise.resolve();
  private closed = false;

  /**
   * Opens `name`. Throws if the Drawing cannot be read or is not a valid
   * Drawing; it is never overwritten then.
   */
  static async open(
    name: string,
    store: DrawingStore,
    core: Core,
    options: OpenDrawingOptions = {},
  ): Promise<OpenDrawing> {
    const bytes = await store.open(name);
    const editor = core.makeEditor();
    try {
      editor.load(bytes);
      const viewport = await store.viewport(name);
      if (viewport) editor.setViewport(viewport);
    } catch (error) {
      editor.free();
      throw error;
    }
    return new OpenDrawing(name, store, editor, options);
  }

  private constructor(
    readonly name: string,
    private readonly store: DrawingStore,
    editor: Editor,
    { autosaveDelay = 1000 }: OpenDrawingOptions,
  ) {
    this.editor = editor;
    this.delay = autosaveDelay;
    this.savedRevision = editor.revision();
    this.savedViewport = editor.viewport();
  }

  /** Call after input or a command reached the editor. Cheap when the Scene did not change (panning, zooming). */
  editorDidChange(): void {
    if (this.closed || !this.hasUnsavedChanges) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => void this.saveNow(), this.delay);
  }

  /** Saves unsaved changes and the viewport now and drops the delayed save. */
  saveNow(): Promise<void> {
    this.stopTimer();
    return this.closed ? this.writes : this.enqueue(this.saveViewport(), this.saveScene());
  }

  /** The Drawing's file exactly as stored, with every change so far saved into it. */
  async file(): Promise<Uint8Array> {
    await this.saveNow();
    return this.store.open(this.name);
  }

  /**
   * Leaves the Drawing and saves everything. An empty Scene is not worth a
   * file: it is not saved, and a file that exists is removed, also if the
   * Drawing was emptied just now. Nothing is written after that.
   */
  close(): Promise<void> {
    if (this.closed) return this.writes;
    this.stopTimer();
    const writes =
      this.editor.elementCount() > 0
        ? this.enqueue(this.saveViewport(), this.saveScene())
        : this.enqueue(() => this.store.delete(this.name).catch(() => {}));
    this.closed = true;
    return writes;
  }

  private get hasUnsavedChanges(): boolean {
    return this.editor.revision() !== this.savedRevision;
  }

  private stopTimer(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
  }

  /** A failure to cache costs only the viewport: the Drawing opens at the default or at an older one. */
  private saveViewport(): () => Promise<void> {
    const viewport = this.editor.viewport();
    if (sameViewport(viewport, this.savedViewport)) return async () => {};
    return async () => {
      try {
        await this.store.saveViewport(viewport, this.name);
        this.savedViewport = viewport;
      } catch {}
    };
  }

  /** A failed save keeps the changes unsaved: the next change or the next `saveNow` tries again. */
  private saveScene(): () => Promise<void> {
    if (!this.hasUnsavedChanges) return async () => {};
    const revision = this.editor.revision();
    const bytes = this.editor.save();
    return async () => {
      try {
        await this.store.save(bytes, this.name);
        this.savedRevision = revision;
      } catch {}
    };
  }

  /** Runs `steps` after the writes asked for before. */
  private enqueue(...steps: (() => Promise<void>)[]): Promise<void> {
    this.writes = this.writes.then(async () => {
      for (const step of steps) await step();
    });
    return this.writes;
  }
}
