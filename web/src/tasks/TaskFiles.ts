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
    readonly store: TaskStore,
    private readonly storage: FileStorage,
  ) {}

  static async open(core: Pick<Core, "newItemId">, storage: FileStorage): Promise<TaskFiles> {
    const device = await deviceId(core, storage);
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
    return new TaskFiles(new TaskStore(device, paths, texts), storage);
  }

  /**
   * Saves what the store changed since the last call. Rejects if a write
   * failed; the changes are then not saved, but later calls go on.
   */
  save(): Promise<void> {
    // What the files hold now, so that a later change can't overtake this save.
    const changes = this.store.takeChanges().map((path) => ({ path, text: this.store.file(path) }));
    const written = this.writing.then(async () => {
      for (const { path, text } of changes) {
        if (text === undefined) await this.storage.remove(path);
        else await this.storage.write(path, new TextEncoder().encode(text));
      }
    });
    this.writing = written.catch(() => {});
    return written;
  }
}

async function deviceId(core: Pick<Core, "newItemId">, storage: FileStorage): Promise<string> {
  const saved = await storage.read(deviceFile);
  if (saved) return decode(saved);
  const id = core.newItemId();
  await storage.write(deviceFile, new TextEncoder().encode(id));
  return id;
}
