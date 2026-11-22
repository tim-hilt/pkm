import { NoteStore } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

/** The Tags file, shared with the Tasks and Bookmarks. */
const TAGS = "Tags/tags.json";

/** What an Image's file name looks like: `<id>.<ext>`. */
const imageName = /^[a-z0-9]+\.(png|jpg|gif|webp)$/;

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/**
 * The Notes and the Images beside them, kept in the Origin Private File System. The core's store works on
 * files in memory; this opens it from `Notes/` and saves what it changed, one
 * write after another; a file the store removed is removed. Files in
 * subdirectories of `Notes/` are not read, and never touched. The Tags file is
 * shared, so the store is given the current one before it changes Tags (`refreshTags`).
 */
export class NoteFiles {
  /** The writes in progress: each save waits for the one before. */
  private writing: Promise<void> = Promise.resolve();

  /** Called after the Tags file was written, for the Tasks to read it again. */
  onTagsSaved: () => void = () => {};

  /** Called after files were written or removed, for Sync to follow. */
  onSaved: () => void = () => {};

  private constructor(
    private current: NoteStore,
    private readonly storage: FileStorage,
  ) {}

  static async open(storage: FileStorage): Promise<NoteFiles> {
    return new NoteFiles(await load(storage), storage);
  }

  /** The store; a new one after `reload`. */
  get store(): NoteStore {
    return this.current;
  }

  /**
   * Starts over from what the files hold now, after Sync wrote or deleted
   * some. What the store changed is saved first; the old store is freed.
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

  /** Gives the store the Tags file as the Tasks left it, so that a new Tag doesn't undo theirs. */
  async refreshTags(): Promise<void> {
    await this.writing;
    const bytes = await this.storage.read(TAGS);
    if (bytes) this.store.loadFile(TAGS, decode(bytes));
  }

  /** Saves what the store changed since the last call. Rejects if a write failed; later calls go on. */
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

/** A store of what the files in `Notes/` hold now. */
async function load(storage: FileStorage): Promise<NoteStore> {
  const root = NoteStore.folder();
  const paths: string[] = [];
  const texts: string[] = [];
  const images: { path: string; bytes: Uint8Array }[] = [];
  for (const { name } of await storage.files(root)) {
    const path = `${root}/${name}`;
    const isImage = imageName.test(name);
    const bytes = isImage || name.endsWith(".md") ? await storage.read(path) : undefined;
    if (!bytes) continue;
    // Images are held too, for the store to read them.
    if (isImage) images.push({ path, bytes });
    else {
      paths.push(path);
      texts.push(decode(bytes));
    }
  }
  const tags = await storage.read(TAGS);
  if (tags) {
    paths.push(TAGS);
    texts.push(decode(tags));
  }
  const store = new NoteStore(paths, texts);
  for (const { path, bytes } of images) store.loadImage(path, bytes);
  return store;
}
