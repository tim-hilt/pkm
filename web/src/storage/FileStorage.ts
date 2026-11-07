/** A file as listed: its name and when it was last written, in milliseconds. */
export interface FileInfo {
  name: string;
  modified: number;
}

/**
 * The little the Drawing store needs of a file system: folders of files, named
 * by `/`-separated paths. Implemented on the Origin Private File System
 * (`OpfsStorage`) and in memory (`MemoryStorage`, for tests and temporary
 * storage). Paths are never empty and never leave the root.
 */
export interface FileStorage {
  /** The files directly in `folder`; none if there is no such folder. */
  files(folder: string): Promise<FileInfo[]>;
  /** The folders directly in `folder`; none if there is no such folder. */
  folders(folder: string): Promise<string[]>;
  /** The file's bytes, or `undefined` if there is no such file. */
  read(path: string): Promise<Uint8Array | undefined>;
  /** Writes the file whole or not at all, making its folders. Sets its modification time to now. */
  write(path: string, bytes: Uint8Array): Promise<void>;
  /** Removes a file, or a folder with all it holds; nothing happens if it is not there. */
  remove(path: string): Promise<void>;
}
