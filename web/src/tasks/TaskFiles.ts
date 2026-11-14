import { type Core, TaskStore } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

/** Where this Device's ID is kept: on the Device only, never in a Synced folder. */
const deviceFile = "Device/device-id";

const decode = (bytes: Uint8Array) => new TextDecoder().decode(bytes);

/**
 * The Tasks, kept in the Origin Private File System. The core's store works on
 * files in memory; this opens it from what is saved and saves what it
 * changed: the Task files, and this Device's History files
 * (`Tasks/.history/<device>-<yyyy-mm>.log`), which the store appends to and so
 * needs to start from. Other Devices' History files are never read.
 */
export class TaskFiles {
  /** The writes in progress: each save waits for the one before. */
  private writing: Promise<void> = Promise.resolve();

  private constructor(
    private current: TaskStore,
    private readonly device: string,
    private readonly storage: FileStorage,
  ) {}

  static async open(core: Pick<Core, "claimItemIds">, storage: FileStorage): Promise<TaskFiles> {
    const device = await deviceId(core, storage);
    return new TaskFiles(await load(device, storage), device, storage);
  }

  /** The store; a new one after `reload`. */
  get store(): TaskStore {
    return this.current;
  }

  /**
   * Starts over from what the files hold now, after Sync wrote or deleted
   * some. What the store changed is saved first, with its pending Close
   * committed, as that can't outlive the store; the old store is freed.
   */
  async reload(): Promise<void> {
    for (;;) {
      const saved = this.saveAll();
      await saved;
      const reloaded = await load(this.device, this.storage);
      // What the store changed while the files were read may be missing from them.
      if (this.saveAll() === saved) {
        this.current.free();
        this.current = reloaded;
        return;
      }
      reloaded.free();
    }
  }

  /**
   * Saves what the store changed since the last call. Rejects if a write
   * failed; the changes are then not saved, but later calls go on.
   */
  save(): Promise<void> {
    // What the files hold now, so that a later change can't overtake this save.
    const changes = this.store.takeChanges().map((path) => ({ path, text: this.store.file(path) }));
    if (changes.length === 0) return this.writing;
    const written = this.writing.then(async () => {
      for (const { path, text } of changes) {
        if (text !== undefined) await this.storage.write(path, new TextEncoder().encode(text));
      }
    });
    this.writing = written.catch(() => {});
    return written;
  }

  /** Commits the pending Close and saves; the writes then in progress, the same as before if there was nothing to save. */
  private saveAll(): Promise<void> {
    this.current.commit();
    void this.save().catch(() => {});
    return this.writing;
  }
}

/** A store opened on the Task files and this Device's History files. */
async function load(device: string, storage: FileStorage): Promise<TaskStore> {
  const folder = TaskStore.folder();
  const paths: string[] = [];
  const texts: string[] = [];
  const take = async (folder: string, wanted: (name: string) => boolean) => {
    for (const { name } of await storage.files(folder)) {
      const bytes = wanted(name) ? await storage.read(`${folder}/${name}`) : undefined;
      if (!bytes) continue;
      paths.push(`${folder}/${name}`);
      texts.push(decode(bytes));
    }
  };
  await take(folder, (name) => name.endsWith(".md"));
  await take(`${folder}/.history`, (name) => name.startsWith(`${device}-`));
  return new TaskStore(device, paths, texts);
}

/** Made on first use, and again if the file holds no Item ID, as on Apple (`pkm_base::device`). */
async function deviceId(core: Pick<Core, "claimItemIds">, storage: FileStorage): Promise<string> {
  const bytes = await storage.read(deviceFile);
  const saved = bytes ? decode(bytes).trim() : "";
  const [made] = core.claimItemIds([saved]);
  if (made === "") return saved;
  await storage.write(deviceFile, new TextEncoder().encode(made));
  return made;
}
