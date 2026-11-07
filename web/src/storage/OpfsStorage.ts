import type { FileInfo, FileStorage } from "./FileStorage";

/**
 * A `FileStorage` on the Origin Private File System: the files are real files
 * of the site's private storage, written whole (a writable stream commits on
 * close, so a failed write leaves the old file).
 */
export class OpfsStorage implements FileStorage {
  constructor(private readonly root: Promise<FileSystemDirectoryHandle>) {}

  /** The root of this site's private file system. */
  static open(): OpfsStorage {
    return new OpfsStorage(navigator.storage.getDirectory());
  }

  async files(folder: string): Promise<FileInfo[]> {
    const directory = await this.directory(folder, false);
    const found: FileInfo[] = [];
    if (!directory) return found;
    for await (const handle of directory.values()) {
      if (handle.kind === "file") {
        const file = await (handle as FileSystemFileHandle).getFile();
        found.push({ name: handle.name, modified: file.lastModified });
      }
    }
    return found;
  }

  async folders(folder: string): Promise<string[]> {
    const directory = await this.directory(folder, false);
    const found: string[] = [];
    if (!directory) return found;
    for await (const handle of directory.values()) {
      if (handle.kind === "directory") found.push(handle.name);
    }
    return found;
  }

  async read(path: string): Promise<Uint8Array | undefined> {
    const handle = await this.file(path, false);
    return handle && new Uint8Array(await (await handle.getFile()).arrayBuffer());
  }

  async write(path: string, bytes: Uint8Array): Promise<void> {
    const handle = await this.file(path, true);
    const writable = await handle!.createWritable();
    try {
      await writable.write(bytes as Uint8Array<ArrayBuffer>);
    } catch (error) {
      await writable.abort();
      throw error;
    }
    await writable.close();
  }

  async remove(path: string): Promise<void> {
    const { parent, name } = split(path);
    const directory = await this.directory(parent, false);
    try {
      await directory?.removeEntry(name, { recursive: true });
    } catch (error) {
      if (!(error instanceof DOMException && error.name === "NotFoundError")) throw error;
    }
  }

  private async directory(
    path: string,
    create: boolean,
  ): Promise<FileSystemDirectoryHandle | undefined> {
    let directory = await this.root;
    for (const part of path.split("/").filter(Boolean)) {
      try {
        directory = await directory.getDirectoryHandle(part, { create });
      } catch (error) {
        if (error instanceof DOMException && error.name === "NotFoundError") return undefined;
        throw error;
      }
    }
    return directory;
  }

  private async file(path: string, create: boolean): Promise<FileSystemFileHandle | undefined> {
    const { parent, name } = split(path);
    const directory = await this.directory(parent, create);
    try {
      return await directory?.getFileHandle(name, { create });
    } catch (error) {
      if (error instanceof DOMException && error.name === "NotFoundError") return undefined;
      throw error;
    }
  }
}

function split(path: string): { parent: string; name: string } {
  const slash = path.lastIndexOf("/");
  return { parent: path.slice(0, Math.max(slash, 0)), name: path.slice(slash + 1) };
}
