import type { FileStorage } from "../storage/FileStorage";

/** A file as Sync lists it: its name, when it was last written (ms) and its size in bytes. */
export interface FolderFile {
  /** The file's name in the folder; in a folder with directories its `/`-separated path. */
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
  /** Only for a folder with directories: every visible directory at any depth, as paths. */
  folders?(): Promise<string[]>;
  /** Only for a folder with directories: makes the directory and the ones above it. */
  makeFolder?(path: string): Promise<void>;
  /** Only for a folder with directories: removes the directory if nothing is in it; whether it is gone. */
  removeFolderIfEmpty?(path: string): Promise<boolean>;
}

/**
 * A folder of plain files in a `FileStorage`: the Books and their state, the
 * Tasks. `nested` for one with directories at any depth, the Notes: its files
 * are named by their path, and it also lists, makes and removes directories.
 */
export class FileFolder implements SyncFolder {
  constructor(
    private readonly storage: FileStorage,
    private readonly folder: string,
    private readonly nested = false,
  ) {}

  async files(): Promise<FolderFile[]> {
    const found: FolderFile[] = [];
    const visit = async (path: string): Promise<void> => {
      const prefix = path && `${path}/`;
      for (const { name, modified, size } of await this.storage.files(this.path(path))) {
        if (isValid(name)) found.push({ file: prefix + name, modified, size });
      }
      if (!this.nested) return;
      for (const name of await this.storage.folders(this.path(path))) {
        if (isValid(name)) await visit(prefix + name);
      }
    };
    await visit("");
    return found;
  }

  async folders(): Promise<string[]> {
    const found: string[] = [];
    const visit = async (path: string): Promise<void> => {
      for (const name of await this.storage.folders(this.path(path))) {
        if (!isValid(name)) continue;
        const child = path ? `${path}/${name}` : name;
        found.push(child);
        await visit(child);
      }
    };
    await visit("");
    return found;
  }

  async makeFolder(path: string): Promise<void> {
    if (this.isValidPath(path)) await this.storage.makeFolder(this.path(path));
  }

  /** Removes the directory if no file or directory is in it; one that is gone counts as removed. */
  async removeFolderIfEmpty(path: string): Promise<boolean> {
    if (!this.isValidPath(path)) return false;
    const inside = this.path(path);
    const [files, folders] = await Promise.all([
      this.storage.files(inside),
      this.storage.folders(inside),
    ]);
    if (files.length > 0 || folders.length > 0) return false;
    await this.storage.remove(inside);
    return true;
  }

  async read(file: string): Promise<Uint8Array | undefined> {
    return this.isValidPath(file) ? this.storage.read(this.path(file)) : undefined;
  }

  /** Writes the file whole if it holds `expected` (none: if there is no such file); whether it did. */
  async writeIfHolding(file: string, bytes: Uint8Array, expected?: Uint8Array): Promise<boolean> {
    if (!(await this.holds(file, expected))) return false;
    await this.storage.write(this.path(file), bytes);
    return true;
  }

  /** Renames the file if it holds `expected` and nothing has the new name; whether it did. */
  async renameIfHolding(from: string, to: string, expected: Uint8Array): Promise<boolean> {
    if (!this.isValidPath(to) || !(await this.holds(from, expected))) return false;
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
    if (!this.isValidPath(file)) return false;
    const current = await this.storage.read(this.path(file));
    if (!expected) return current === undefined;
    return current !== undefined && sameBytes(current, expected);
  }

  private path(file: string): string {
    return file ? `${this.folder}/${file}` : this.folder;
  }

  /** A name, or in a folder with directories a path of them. */
  private isValidPath(path: string): boolean {
    return this.nested ? path.split("/").every(isValid) : isValid(path);
  }
}

const isValid = (name: string) => name !== "" && !name.includes("/") && !name.startsWith(".");

const sameBytes = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((byte, i) => byte === b[i]);
