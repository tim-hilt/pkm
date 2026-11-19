import { NoteStore } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

/** What an Image's file name looks like: `<id>.<ext>`. */
const imageName = /^[a-z0-9]+\.(png|jpg|gif|webp)$/;

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/**
 * The Notes and the Images beside them, kept in the Origin Private File System. The core's store works on
 * files in memory; this opens it from `Notes/` and saves what it changed, one
 * write after another; a file the store removed is removed. Folders are kept
 * as folders, empty ones too: after each change the ones the store has and
 * the storage hasn't are made, and the ones it no longer has are removed.
 */
export class NoteFiles {
  /** The writes in progress: each save waits for the one before. */
  private writing: Promise<void> = Promise.resolve();

  private constructor(
    readonly store: NoteStore,
    private readonly storage: FileStorage,
    /** The Folders the storage holds, as the store's paths. */
    private held: Set<string>,
  ) {}

  static async open(storage: FileStorage): Promise<NoteFiles> {
    const root = NoteStore.folder();
    const paths: string[] = [];
    const texts: string[] = [];
    const folders: string[] = [];
    const images: { path: string; bytes: Uint8Array }[] = [];
    const visit = async (folder: string): Promise<void> => {
      for (const { name } of await storage.files(folder)) {
        const path = `${folder}/${name}`;
        const isImage = imageName.test(name);
        const bytes = isImage || name.endsWith(".md") ? await storage.read(path) : undefined;
        if (!bytes) continue;
        // Images are held too, for the store to Move them with their Note.
        if (isImage) images.push({ path, bytes });
        else {
          paths.push(path);
          texts.push(decode(bytes));
        }
      }
      for (const name of await storage.folders(folder)) {
        folders.push(`${folder}/${name}`);
        await visit(`${folder}/${name}`);
      }
    };
    await visit(root);
    const store = new NoteStore(paths, texts, folders);
    for (const { path, bytes } of images) store.loadImage(path, bytes);
    return new NoteFiles(store, storage, new Set(folderPaths(store)));
  }

  /** Saves what the store changed since the last call. Rejects if a write failed; later calls go on. */
  save(): Promise<void> {
    // What the files hold now, so that a later change can't overtake this save.
    const changes = this.store
      .takeChanges()
      .map((path) => ({ path, bytes: this.store.fileBytes(path) }));
    const now = new Set(folderPaths(this.store));
    const gone = [...this.held].filter((path) => !now.has(path));
    const made = [...now].filter((path) => !this.held.has(path));
    this.held = now;
    if (changes.length === 0 && gone.length === 0 && made.length === 0) return this.writing;
    const root = NoteStore.folder();
    const written = this.writing.then(async () => {
      for (const { path, bytes } of changes) {
        if (bytes === undefined) await this.storage.remove(path);
        else await this.storage.write(path, bytes);
      }
      // After the files, which a Moved Folder's old place held until now.
      for (const path of gone) await this.storage.remove(`${root}/${path}`);
      for (const path of made) await this.storage.makeFolder(`${root}/${path}`);
    });
    this.writing = written.catch(() => {});
    return written;
  }
}

/** The paths of every Folder the store has. */
function folderPaths(store: NoteStore): string[] {
  return store.folders().map((row) => {
    const { path } = row;
    row.free();
    return path;
  });
}
