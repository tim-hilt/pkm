import type { Core } from "../../core/Core";
import { DrawingStore, type Entry, InvalidName, NameTaken } from "./DrawingStore";
import { OpenDrawing, type OpenDrawingOptions } from "./OpenDrawing";
import type { Thumbnails } from "./Thumbnails";

/**
 * The Gallery: the Drawings of a `DrawingStore` and which of them is open.
 * Opening one shows it in the Canvas; going back saves it and lists it again.
 * A port of the Swift `Gallery`: whoever shows it listens with `onChange` and
 * reads the state again.
 */
export class Gallery {
  /** The Drawings, last modified first. */
  drawings: Entry[] = [];
  /** The Drawing shown in the Canvas, or `undefined` while the Gallery is shown. */
  open: OpenDrawing | undefined;
  /** What went wrong with the last attempt to change a Drawing, to tell the user. */
  failure: string | undefined;
  /** Called after any change of what the properties above hold. */
  onChange: () => void = () => {};

  private constructor(
    private readonly store: DrawingStore,
    private readonly core: Core,
    /** The pictures of the Drawings for their cards. */
    readonly thumbnails: Thumbnails,
    private readonly options: OpenDrawingOptions,
  ) {}

  /** Lists the Drawings, after clearing out the caches of Drawings that are gone. */
  static async start(
    store: DrawingStore,
    core: Core,
    thumbnails: Thumbnails,
    options: OpenDrawingOptions = {},
  ): Promise<Gallery> {
    const gallery = new Gallery(store, core, thumbnails, options);
    await store.removeOrphanedCaches().catch(() => {});
    await gallery.reload();
    return gallery;
  }

  async reload(): Promise<void> {
    this.drawings = await this.store.list().catch(() => []);
    this.onChange();
  }

  /** Makes a new empty Drawing and opens it. */
  createDrawing(): Promise<void> {
    return this.attempt("Could not create a Drawing.", async () => {
      const editor = this.core.makeEditor();
      const empty = editor.save();
      editor.free();
      await this.show(await this.store.create(empty));
    });
  }

  /** Gives a Drawing another name; tells the user if that name is taken or cannot be used. */
  renameDrawing(name: string, newName: string): Promise<void> {
    return this.attempt(`Could not rename “${name}”.`, async () => {
      await this.store.rename(name, newName);
      await this.reload();
    });
  }

  duplicateDrawing(name: string): Promise<void> {
    return this.attempt(`Could not duplicate “${name}”.`, async () => {
      await this.store.duplicate(name);
      await this.reload();
    });
  }

  /** Deletes a Drawing for good; the caller confirms with the user first. */
  deleteDrawing(name: string): Promise<void> {
    return this.attempt(`Could not delete “${name}”.`, async () => {
      await this.store.delete(name);
      await this.reload();
    });
  }

  openDrawing(name: string): Promise<void> {
    return this.attempt(`Could not open “${name}”.`, () => this.show(name));
  }

  /** Saves the open Drawing and returns to the Gallery. */
  async close(): Promise<void> {
    const drawing = this.open;
    if (!drawing) return;
    this.open = undefined;
    // Whoever shows the Drawing lets go of its editor before that is freed.
    this.onChange();
    await drawing.close();
    drawing.editor.free();
    await this.reload();
  }

  dismissFailure(): void {
    this.failure = undefined;
    this.onChange();
  }

  private async show(name: string): Promise<void> {
    this.open = await OpenDrawing.open(name, this.store, this.core, this.options);
    this.onChange();
  }

  private async attempt(message: string, work: () => Promise<void>): Promise<void> {
    this.failure = undefined;
    try {
      await work();
    } catch (error) {
      this.failure =
        error instanceof NameTaken
          ? `“${error.name}” already exists. Choose another name.`
          : error instanceof InvalidName
            ? "That name cannot be used. Choose another name."
            : message;
      this.onChange();
    }
  }
}
