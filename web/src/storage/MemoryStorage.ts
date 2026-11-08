import type { FileInfo, FileStorage } from "./FileStorage";

/**
 * A `FileStorage` in memory: for tests. Modification times never repeat or go back, so that "newest first"
 * is decided even when writes come within a millisecond.
 */
export class MemoryStorage implements FileStorage {
  private readonly contents = new Map<string, { bytes: Uint8Array; modified: number }>();
  private last = 0;

  constructor(private readonly clock: () => number = Date.now) {}

  async files(folder: string): Promise<FileInfo[]> {
    const prefix = `${folder}/`;
    const found: FileInfo[] = [];
    for (const [path, { modified, bytes }] of this.contents) {
      const name = path.slice(prefix.length);
      if (path.startsWith(prefix) && !name.includes("/"))
        found.push({ name, modified, size: bytes.length });
    }
    return found;
  }

  async folders(folder: string): Promise<string[]> {
    const prefix = `${folder}/`;
    const found = new Set<string>();
    for (const path of this.contents.keys()) {
      const [first, ...rest] = path.slice(prefix.length).split("/");
      if (path.startsWith(prefix) && rest.length > 0) found.add(first);
    }
    return [...found];
  }

  async read(path: string): Promise<Uint8Array | undefined> {
    const file = this.contents.get(path);
    return file && file.bytes.slice();
  }

  async write(path: string, bytes: Uint8Array): Promise<void> {
    this.last = Math.max(this.clock(), this.last + 1);
    this.contents.set(path, { bytes: bytes.slice(), modified: this.last });
  }

  async remove(path: string): Promise<void> {
    this.contents.delete(path);
    for (const other of this.contents.keys()) {
      if (other.startsWith(`${path}/`)) this.contents.delete(other);
    }
  }
}
