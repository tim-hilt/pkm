import type { Viewport } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";

/** A Drawing as listed in the Gallery. */
export interface Entry {
  name: string;
  /** Milliseconds since the epoch. */
  modified: number;
}

/** A Drawing's file as Cloud Backing lists it. */
export interface DrawingFile {
  /** `<name>.excalidraw`. */
  file: string;
  /** Milliseconds since the epoch. */
  modified: number;
  /** In bytes. */
  size: number;
}

/** A name that is empty or would put the file somewhere else than in the store's folder. */
export class InvalidName extends Error {
  constructor(readonly name: string) {
    super(`“${name}” cannot be used as a name`);
  }
}

/** Another Drawing already has the name. */
export class NameTaken extends Error {
  constructor(readonly name: string) {
    super(`“${name}” already exists`);
  }
}

/** There is no Drawing of that name. */
export class NotFound extends Error {
  constructor(readonly name: string) {
    super(`there is no Drawing “${name}”`);
  }
}

const fileExtension = ".excalidraw";
const drawings = "Drawings";
const caches = "Caches";
const viewportFile = "viewport.json";
const thumbnailFile = /^(thumbnail(?:-dark)?)-(\d+)\.png$/;

/**
 * The Drawings of this Device: `<name>.excalidraw` files in one folder of a
 * `FileStorage` (the Origin Private File System in the app; ADR-0003). The
 * file name is the Drawing's name. All file I/O lives here; the core only
 * sees bytes. A port of the Swift `DrawingStore`.
 *
 * Whatever the app derives from a Drawing (viewport, thumbnail) is cached
 * beside the folder, in one subfolder per Drawing, so that the Drawings stay
 * plain files. A cache follows its Drawing when that is renamed and goes when
 * it is deleted; it is never copied. Nothing here watches the storage: only
 * Cloud Backing changes it besides the app, through the guarded writes below.
 *
 * Writes run one after the other, so that a Sync write checking what a file
 * holds never interleaves with a save.
 */
export class DrawingStore {
  /** Called after every change the app makes to a Drawing (not the ones Sync makes), so that Cloud Backing can Sync it. */
  onLocalChange?: () => void;
  private tail: Promise<unknown> = Promise.resolve();

  /** `onSave` is called after every write of a Drawing (the first one asks the browser for persistent storage). */
  constructor(
    private readonly storage: FileStorage,
    private readonly onSave?: () => void,
  ) {}

  /** The Drawings, last modified first. */
  async list(): Promise<Entry[]> {
    const files = await this.storage.files(drawings);
    return files
      .filter(({ name }) => name.endsWith(fileExtension))
      .map(({ name, modified }) => ({ name: name.slice(0, -fileExtension.length), modified }))
      .sort((a, b) => b.modified - a.modified || (a.name < b.name ? -1 : a.name > b.name ? 1 : 0));
  }

  /**
   * Makes a new Drawing named "Untitled <date time>" (made distinct if that
   * name is taken) holding `contents`, and returns its name.
   */
  create(contents: Uint8Array, at: Date = new Date()): Promise<string> {
    return this.add(contents, `Untitled ${format(at)}`);
  }

  /**
   * Makes a new Drawing holding `contents` named `name`, or "<name> 2",
   * "<name> 3", … if that is taken, and returns the name. Never replaces
   * another Drawing.
   */
  add(contents: Uint8Array, name: string): Promise<string> {
    return this.exclusively(async () => {
      const distinct = await this.distinctName(name.trim());
      // Left by a Drawing of that name that went without the store knowing.
      await this.storage.remove(cacheFolder(distinct));
      await this.write(contents, distinct);
      this.onLocalChange?.();
      return distinct;
    });
  }

  /** The bytes of the Drawing. */
  async open(name: string): Promise<Uint8Array> {
    const bytes = await this.storage.read(path(name));
    if (!bytes) throw new NotFound(name);
    return bytes;
  }

  /** Writes the Drawing: the file is whole with either the old or the new contents. */
  save(contents: Uint8Array, name: string): Promise<void> {
    return this.exclusively(async () => {
      await this.write(contents, name);
      this.onLocalChange?.();
    });
  }

  /**
   * Gives the Drawing another name and returns it (the given one without
   * surrounding whitespace). Throws `NameTaken` rather than replace another
   * Drawing; the same name is fine.
   */
  rename(name: string, newName: string): Promise<string> {
    return this.exclusively(async () => {
      newName = newName.trim();
      const target = path(newName);
      const contents = await this.open(name);
      if (newName === name) return name;
      if ((await this.names()).has(newName)) throw new NameTaken(newName);
      await this.storage.write(target, contents);
      await this.storage.remove(path(name));
      await this.moveCaches(name, newName);
      this.onLocalChange?.();
      return newName;
    });
  }

  /** Copies the Drawing under a distinct name ("<name> copy", "<name> copy 2", …) and returns it. */
  async duplicate(name: string): Promise<string> {
    return this.add(await this.open(name), `${name} copy`);
  }

  /** Removes the Drawing and its caches. There is no way back. */
  delete(name: string): Promise<void> {
    return this.exclusively(async () => {
      if (!(await this.names()).has(validated(name))) throw new NotFound(name);
      await this.storage.remove(path(name));
      await this.storage.remove(cacheFolder(name));
      this.onLocalChange?.();
    });
  }

  // What Cloud Backing needs: the Drawings as files, and writes that happen
  // only if the file still holds what Sync read. Names that cannot be a
  // Drawing's here (empty, with a slash, a leading dot) are not listed, read
  // or written; the writes then report that nothing happened.

  /** The Drawings' files with their modification times and sizes. */
  async files(): Promise<DrawingFile[]> {
    const files = await this.storage.files(drawings);
    return files
      .filter(({ name }) => isDrawingFile(name))
      .map(({ name, modified, size }) => ({ file: name, modified, size }));
  }

  /** The bytes of the Drawing file, `undefined` if there is none. */
  async read(file: string): Promise<Uint8Array | undefined> {
    return isDrawingFile(file) ? this.storage.read(`${drawings}/${file}`) : undefined;
  }

  /** Writes the Drawing file whole if it holds `expected` (none: if there is no such file); whether it did. Not a change by the user. */
  writeIfHolding(file: string, bytes: Uint8Array, expected?: Uint8Array): Promise<boolean> {
    return this.exclusively(async () => {
      if (!(await this.holds(file, expected))) return false;
      const name = nameOf(file);
      // Left by a Drawing of that name that went without the store knowing.
      if (!expected) await this.storage.remove(cacheFolder(name));
      await this.storage.write(`${drawings}/${file}`, bytes);
      return true;
    });
  }

  /** Renames the Drawing file if it holds `expected` and nothing has the new name; whether it did. */
  renameIfHolding(from: string, to: string, expected: Uint8Array): Promise<boolean> {
    return this.exclusively(async () => {
      if (!isDrawingFile(to) || !(await this.holds(from, expected))) return false;
      if (await this.storage.read(`${drawings}/${to}`)) return false;
      await this.storage.write(`${drawings}/${to}`, expected);
      await this.storage.remove(`${drawings}/${from}`);
      await this.moveCaches(nameOf(from), nameOf(to));
      return true;
    });
  }

  /** Deletes the Drawing file and its caches if it holds `expected`; whether it did. */
  deleteIfHolding(file: string, expected: Uint8Array): Promise<boolean> {
    return this.exclusively(async () => {
      if (!(await this.holds(file, expected))) return false;
      await this.storage.remove(`${drawings}/${file}`);
      await this.storage.remove(cacheFolder(nameOf(file)));
      return true;
    });
  }

  private async holds(file: string, expected: Uint8Array | undefined): Promise<boolean> {
    if (!isDrawingFile(file)) return false;
    const current = await this.storage.read(`${drawings}/${file}`);
    if (!expected) return current === undefined;
    return current !== undefined && sameBytes(current, expected);
  }

  private async write(contents: Uint8Array, name: string): Promise<void> {
    await this.storage.write(path(name), contents);
    this.onSave?.();
  }

  /** Runs `work` after everything before it, whether that worked or not. */
  private exclusively<T>(work: () => Promise<T>): Promise<T> {
    const run = this.tail.then(work, work);
    this.tail = run.catch(() => {});
    return run;
  }

  /**
   * Removes the caches of Drawings that are gone without the store's knowing.
   * Only frees space: a Drawing made under such a name later never picks up
   * its caches anyway.
   */
  async removeOrphanedCaches(): Promise<void> {
    const names = await this.names();
    for (const folder of await this.storage.folders(caches)) {
      if (!names.has(folder)) await this.storage.remove(`${caches}/${folder}`);
    }
  }

  // Thumbnails: one file per appearance, named for the modification time of
  // the Drawing it shows. It is current exactly when that is still the
  // Drawing's: whatever changes the file makes it stale.

  /**
   * The cached thumbnail (PNG) of the Drawing as it is listed in `entry`, in
   * the light or dark appearance (ADR-0010), or `undefined` if there is none
   * or the Drawing changed since it was made.
   */
  async thumbnail(entry: Entry, dark = false): Promise<Uint8Array | undefined> {
    return this.storage.read(`${cacheFolder(entry.name)}/${thumbnailName(entry, dark)}`);
  }

  /**
   * Caches `png` as the thumbnail of the Drawing as it is listed in `entry`
   * for the light or dark appearance. Not a change of the Drawing.
   */
  async saveThumbnail(png: Uint8Array, entry: Entry, dark = false): Promise<void> {
    const folder = cacheFolder(entry.name);
    const kind = thumbnailKind(dark);
    for (const { name } of await this.storage.files(folder)) {
      if (thumbnailFile.exec(name)?.[1] === kind) await this.storage.remove(`${folder}/${name}`);
    }
    await this.storage.write(`${folder}/${thumbnailName(entry, dark)}`, png);
  }

  // Viewport

  /**
   * The viewport (scroll and zoom) the Drawing was left at, or `undefined` if
   * none was cached or it cannot be read. It is the app's own state and not
   * part of the Drawing, so it lives in the cache and never in the file.
   */
  async viewport(name: string): Promise<Viewport | undefined> {
    if (!isValid(name)) return undefined;
    const bytes = await this.storage.read(`${cacheFolder(name)}/${viewportFile}`);
    if (!bytes) return undefined;
    try {
      const { originX, originY, zoom } = JSON.parse(new TextDecoder().decode(bytes));
      return [originX, originY, zoom].every(Number.isFinite)
        ? { originX, originY, zoom }
        : undefined;
    } catch {
      return undefined;
    }
  }

  /** Caches `viewport` as the one to open the Drawing at. Not a change of the Drawing. */
  async saveViewport({ originX, originY, zoom }: Viewport, name: string): Promise<void> {
    const json = JSON.stringify({ originX, originY, zoom });
    await this.storage.write(`${cacheFolder(validated(name))}/${viewportFile}`, encode(json));
  }

  /** `base`, or the first of "<base> 2", "<base> 3", … no Drawing has. */
  private async distinctName(base: string): Promise<string> {
    validated(base);
    const taken = await this.names();
    let name = base;
    for (let number = 2; taken.has(name); number++) name = `${base} ${number}`;
    return name;
  }

  private async names(): Promise<Set<string>> {
    return new Set((await this.list()).map((entry) => entry.name));
  }

  /** Caches can be made again, so they are moved file by file; the thumbnails' stamps follow the new file. */
  private async moveCaches(name: string, newName: string): Promise<void> {
    const source = cacheFolder(name);
    const target = cacheFolder(newName);
    await this.storage.remove(target);
    const modified = (await this.list()).find((entry) => entry.name === newName)?.modified;
    try {
      for (const { name: file } of await this.storage.files(source)) {
        const bytes = await this.storage.read(`${source}/${file}`);
        const thumbnail = thumbnailFile.exec(file);
        const moved = thumbnail ? `${thumbnail[1]}-${modified}.png` : file;
        if (bytes) await this.storage.write(`${target}/${moved}`, bytes);
      }
    } catch {
      await this.storage.remove(target);
    }
    await this.storage.remove(source);
  }
}

const encode = (text: string) => new TextEncoder().encode(text);

const isDrawingFile = (file: string) =>
  file.endsWith(fileExtension) && isValid(file.slice(0, -fileExtension.length));
const nameOf = (file: string) => file.slice(0, -fileExtension.length);
const sameBytes = (a: Uint8Array, b: Uint8Array) =>
  a.length === b.length && a.every((byte, i) => byte === b[i]);

const isValid = (name: string) => name !== "" && !name.includes("/") && !name.startsWith(".");

function validated(name: string): string {
  if (!isValid(name)) throw new InvalidName(name);
  return name;
}

const path = (name: string) => `${drawings}/${validated(name)}${fileExtension}`;
const cacheFolder = (name: string) => `${caches}/${name}`;
const thumbnailKind = (dark: boolean) => (dark ? "thumbnail-dark" : "thumbnail");
const thumbnailName = (entry: Entry, dark: boolean) =>
  `${thumbnailKind(dark)}-${entry.modified}.png`;

/** `2026-11-03 19.27.31`: no colons, which file systems show as slashes. */
function format(date: Date): string {
  const two = (n: number) => String(n).padStart(2, "0");
  return (
    `${date.getFullYear()}-${two(date.getMonth() + 1)}-${two(date.getDate())} ` +
    `${two(date.getHours())}.${two(date.getMinutes())}.${two(date.getSeconds())}`
  );
}
