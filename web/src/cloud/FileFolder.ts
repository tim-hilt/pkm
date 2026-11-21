import type { FileStorage } from "../storage/FileStorage";

/** A file as Sync lists it: its name, when it was last written (ms) and its size in bytes. */
export interface FolderFile {
  /** The file's name in the folder, not its path. */
  file: string;
  /** Milliseconds since the epoch. */
  modified: number;
  /** In bytes. */
  size: number;
}

/**
 * A folder of plain files that Sync reads and writes, with guarded writes:
 * a write, rename or delete happens only while the file still holds what Sync
 * read. The Drawings folder is one (`DrawingStore`); the Books folder and its
 * hidden `.state` folder are others.
 */
export interface SyncFolder {
  files(): Promise<FolderFile[]>;
  read(file: string): Promise<Uint8Array | undefined>;
  writeIfHolding(file: string, bytes: Uint8Array, expected?: Uint8Array): Promise<boolean>;
  renameIfHolding(from: string, to: string, expected: Uint8Array): Promise<boolean>;
  deleteIfHolding(file: string, expected: Uint8Array): Promise<boolean>;
}

/** A plain folder of files in a `FileStorage`: the Books and their state. */
export class FileFolder implements SyncFolder {
  constructor(
    private readonly storage: FileStorage,
    private readonly folder: string,
  ) {}

  async files(): Promise<FolderFile[]> {
    return (await this.storage.files(this.folder))
      .filter(({ name }) => isValid(name))
      .map(({ name, modified, size }) => ({ file: name, modified, size }));
  }

  async read(file: string): Promise<Uint8Array | undefined> {
    return isValid(file) ? this.storage.read(this.path(file)) : undefined;
  }

  /** Writes the file whole if it holds `expected` (none: if there is no such file); whether it did. */
  async writeIfHolding(file: string, bytes: Uint8Array, expected?: Uint8Array): Promise<boolean> {
    if (!(await this.holds(file, expected))) return false;
    await this.storage.write(this.path(file), bytes);
    return true;
  }

  /** Renames the file if it holds `expected` and nothing has the new name; whether it did. */
  async renameIfHolding(from: string, to: string, expected: Uint8Array): Promise<boolean> {
    if (!isValid(to) || !(await this.holds(from, expected))) return false;
    if (await this.storage.read(this.path(to))) return false;
    await this.storage.write(this.path(to), expected);
    await this.storage.remove(this.path(from));
    return true;
  }

  /** Deletes the file if it holds `expected`; whether it did. */
  async deleteIfHolding(file: string, expected: Uint8Array): Promise<boolean> {
    if (!(await this.holds(file, expected))) return false;
    await this.storage.remove(this.path(file));
    return true;
  }

  private async holds(file: string, expected: Uint8Array | undefined): Promise<boolean> {
    if (!isValid(file)) return false;
    const current = await this.storage.read(this.path(file));
    if (!expected) return current === undefined;
    return current !== undefined && sameBytes(current, expected);
  }

  private path(file: string): string {
    return `${this.folder}/${file}`;
  }
}

const isValid = (name: string) => name !== "" && !name.includes("/") && !name.startsWith(".");

const sameBytes = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((byte, i) => byte === b[i]);
