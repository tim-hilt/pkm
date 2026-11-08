import type { Core, Editor, Viewport } from "../core/Core";
import type { DrawingStore } from "./DrawingStore";

export interface OpenDrawingOptions {
  /** How long after the last change the Drawing is saved; milliseconds. */
  autosaveDelay?: number;
}

const fileOf = (name: string) => `${name}.excalidraw`;

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
 * Sync changes the Drawing's file behind its back (`syncDidChange`): the
 * bytes are merged into the Editor instead of reopening, a rename is followed
 * and a delete is ignored, so that the next save writes the Drawing again
 * (the user's edit wins over a delete).
 *
 * Writes run one after the other, so that a save never overtakes the one
 * before it or the removal of an emptied Drawing.
 */
export class OpenDrawing {
  readonly editor: Editor;
  /** Called after a Synced change reached the Editor, so that whatever shows it can show it again. */
  onMerged?: () => void;
  private currentName: string;
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
    return new OpenDrawing(name, store, core, editor, options);
  }

  private constructor(
    name: string,
    private readonly store: DrawingStore,
    private readonly core: Core,
    editor: Editor,
    { autosaveDelay = 1000 }: OpenDrawingOptions,
  ) {
    this.currentName = name;
    this.editor = editor;
    this.delay = autosaveDelay;
    this.savedRevision = editor.revision();
    this.savedViewport = editor.viewport();
  }

  /** The Drawing's name; it follows a rename by Sync. */
  get name(): string {
    return this.currentName;
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

  /**
   * Sync wrote or deleted the files of the Drawings `names` (a rename as the
   * old and the new name). Takes in what Sync did to this Drawing: the changed
   * file is merged into the Editor (no undo step; Selection, Text view and
   * viewport stay), a rename is followed, a delete leaves the Drawing open.
   * Runs after the saves asked for before.
   */
  syncDidChange(names: string[]): Promise<void> {
    if (this.closed) return this.writes;
    return this.enqueue(() => this.followSync(names));
  }

  private async followSync(names: string[]): Promise<void> {
    if (this.closed || !names.includes(this.name)) return;
    let bytes = await this.store.read(fileOf(this.name));
    if (!bytes) {
      // Renamed if exactly one of the other files Sync wrote is this Drawing; otherwise deleted.
      const renamed = await this.renamedTo(names);
      if (!renamed) return;
      this.currentName = renamed.name;
      bytes = renamed.bytes;
    }
    const wasSaved = !this.hasUnsavedChanges;
    try {
      this.editor.mergeSynced(bytes);
    } catch {
      return; // Not a Drawing: left alone, our next save replaces it.
    }
    // With nothing of ours unsaved the Scene now is what the file holds;
    // otherwise the next save writes the merged Scene.
    if (wasSaved) this.savedRevision = this.editor.revision();
    this.onMerged?.();
  }

  private async renamedTo(
    names: string[],
  ): Promise<{ name: string; bytes: Uint8Array } | undefined> {
    const ours = this.editor.save();
    const same: { name: string; bytes: Uint8Array }[] = [];
    for (const name of names) {
      if (name === this.name) continue;
      const bytes = await this.store.read(fileOf(name));
      if (bytes && this.core.areSameDrawing(ours, bytes)) same.push({ name, bytes });
    }
    return same.length === 1 ? same[0] : undefined;
  }

  /** The Drawing's file exactly as stored, with every change so far saved into it. */
  async file(): Promise<Uint8Array> {
    await this.saveNow();
    return this.store.open(this.name);
  }

  /**
   * Leaves the Drawing and saves everything: a text being edited is committed
   * as typed (its Text view is gone with the Canvas, before it could end the
   * editing). An empty Scene is not worth a file: it is not saved, and a file
   * that exists is removed, also if the Drawing was emptied just now. Nothing
   * is written after that.
   */
  close(): Promise<void> {
    if (this.closed) return this.writes;
    this.editor.commitText();
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
