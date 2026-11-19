import { NoteStore } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/**
 * The Notes, kept in the Origin Private File System. The core's store works on
 * files in memory; this opens it from `Notes/` and saves what it changed, one
 * write after another; a file the store removed is removed.
 */
export class NoteFiles {
  /** The writes in progress: each save waits for the one before. */
  private writing: Promise<void> = Promise.resolve();

  private constructor(
    readonly store: NoteStore,
    private readonly storage: FileStorage,
  ) {}

  static async open(storage: FileStorage): Promise<NoteFiles> {
    const folder = NoteStore.folder();
    const paths: string[] = [];
    const texts: string[] = [];
    for (const { name } of await storage.files(folder)) {
      const bytes = name.endsWith(".md") ? await storage.read(`${folder}/${name}`) : undefined;
      if (!bytes) continue;
      paths.push(`${folder}/${name}`);
      texts.push(decode(bytes));
    }
    return new NoteFiles(new NoteStore(paths, texts), storage);
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
    });
    this.writing = written.catch(() => {});
    return written;
  }
}
