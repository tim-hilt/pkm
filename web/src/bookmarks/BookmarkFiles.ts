import { BookmarkStore } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/** What an image's file name looks like: `<id>.<ext>`, with no way out of the Bookmarks folder. */
const imageName = /^[a-z0-9]+\.(png|jpg|gif|webp)$/;

const TAGS = "Tags/tags.json";
/** The Highlight Colors, shared with Books (ADR-0034). */
const COLORS = "Colors/colors.json";

/**
 * The Bookmarks, kept in the Origin Private File System: `Bookmarks/<id>.md`, their state files
 * beside them, and the images. The core's store works on text files in memory; this opens it from
 * them and saves what it changed, one write after another. Images are not held in memory: they are
 * read from storage when shown, and those the store wrote are let go once saved. The Tags file is shared with the Tasks, so the store is given
 * the current one before it changes Tags (`refreshTags`).
 */
export class BookmarkFiles {
  /** The writes in progress: each save waits for the one before. */
  private writing: Promise<void> = Promise.resolve();

  /** Called after the Tags file was written, for the Tasks to read it again. */
  onTagsSaved: () => void = () => {};

  /** Called after files were written or removed, for Sync to follow. */
  onSaved: () => void = () => {};

  private constructor(
    private current: BookmarkStore,
    private readonly storage: FileStorage,
  ) {}

  static async open(storage: FileStorage): Promise<BookmarkFiles> {
    return new BookmarkFiles(await load(storage), storage);
  }

  /** The store; a new one after `reload`. */
  get store(): BookmarkStore {
    return this.current;
  }

  /**
   * Starts over from what the files hold now, after Sync wrote or deleted some. What the store
   * changed is saved first; the old store is freed.
   */
  async reload(): Promise<void> {
    for (;;) {
      const saved = this.saveAll();
      await saved;
      const reloaded = await load(this.storage);
      // What the store changed while the files were read may be missing from them.
      if (this.saveAll() === saved) {
        this.current.free();
        this.current = reloaded;
        return;
      }
      reloaded.free();
    }
  }

  /** Settles when what was changed is saved. */
  get saving(): Promise<void> {
    return this.writing;
  }

  /** The bytes of the image `name` beside the Bookmarks; `undefined` if there is none. */
  async readImage(name: string): Promise<Uint8Array | undefined> {
    if (!imageName.test(name)) return undefined;
    const path = `${BookmarkStore.folder()}/${name}`;
    // Written but perhaps not saved yet.
    return this.store.fileBytes(path) ?? this.storage.read(path);
  }

  /** The Highlight Colors file as Sync or Books left it; `undefined` if there is none. */
  readColors(): Promise<Uint8Array | undefined> {
    return this.storage.read(COLORS);
  }

  /** Gives the store the Tags file as the Tasks left it, so that a new Tag doesn't undo theirs. */
  async refreshTags(): Promise<void> {
    await this.writing;
    const bytes = await this.storage.read(TAGS);
    if (bytes) this.store.loadFile(TAGS, decode(bytes));
  }

  /**
   * Saves what the store changed since the last call. Rejects if a write failed; the changes are
   * then not saved, but later calls go on.
   */
  save(): Promise<void> {
    // What the files hold now, so that a later change can't overtake this save.
    const changes = this.store
      .takeChanges()
      .map((path) => ({ path, bytes: this.store.fileBytes(path) }));
    if (changes.length === 0) return this.writing;
    const written = this.writing.then(async () => {
      for (const { path, bytes } of changes) {
        if (bytes === undefined) await this.storage.remove(path);
        else await this.storage.write(path, bytes);
        if (bytes && isImage(path)) this.store.unloadFile(path);
      }
      if (changes.some(({ path }) => path === TAGS)) this.onTagsSaved();
      this.onSaved();
    });
    this.writing = written.catch(() => {});
    return written;
  }

  /** Saves what changed; the writes then in progress, the same as before if there was nothing to save. */
  private saveAll(): Promise<void> {
    void this.save().catch(() => {});
    return this.writing;
  }
}

/** Whether `path` is an image beside the Bookmarks. */
const isImage = (path: string) =>
  path.startsWith(`${BookmarkStore.folder()}/`) && imageName.test(path.split("/")[1] ?? "");

/** A store of what `Bookmarks/` and the Tags file hold now. */
async function load(storage: FileStorage): Promise<BookmarkStore> {
  const folder = BookmarkStore.folder();
  const paths: string[] = [];
  const texts: string[] = [];
  const take = async (from: string, wanted: (name: string) => boolean) => {
    for (const { name } of await storage.files(from)) {
      const bytes = wanted(name) ? await storage.read(`${from}/${name}`) : undefined;
      if (!bytes) continue;
      paths.push(`${from}/${name}`);
      texts.push(decode(bytes));
    }
  };
  await take(folder, (name) => name.endsWith(".md"));
  await take(`${folder}/.state`, (name) => name.endsWith(".json"));
  await take("Tags", (name) => name === "tags.json");
  return new BookmarkStore(paths, texts);
}
