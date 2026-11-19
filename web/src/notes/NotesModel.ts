import { imageTargets } from "../core/Core";
import type { NoteFiles } from "./NoteFiles";

/** A Note as a row of the list shows it: a plain copy, as the core's objects must be freed. */
export interface ListedNote {
  id: string;
  title: string;
  /** The Body's line to show when the Title is empty. */
  preview: string;
}

/** A Note that Search found; a plain copy of the core's result. */
export interface SearchHit {
  id: string;
  title: string;
  /** The path of its Folder; empty for the top level. */
  folder: string;
  /** Around the match in the Body, as typed. */
  snippet: string;
  /** The match in `snippet`: `matchStart` up to `matchEnd`. */
  matchStart: number;
  matchEnd: number;
  titleMatched: boolean;
}

/** A Folder as a row of the outline: the tree flattened, parents before their Folders. */
export interface FolderRow {
  name: string;
  /** Names from the top level, joined by `/`. */
  path: string;
  /** How many Folders it is in; 0 at the top level. */
  depth: number;
}

/** A Note's Title and Body as typed. */
export interface NoteText {
  title: string;
  body: string;
}

/**
 * The Notes Context's state: the Folder outline, the selected Folder's Notes
 * and the open Note, from the core's store, saved to storage after each change. It only forwards to the store;
 * every Notes rule lives in the core. A port of the Swift `NotesModel`.
 */
export class NotesModel {
  /** Every Folder, as the outline shows them. */
  folders: FolderRow[] = [];
  /** The Notes of the selected Folder, newest changed first. */
  rows: ListedNote[] = [];
  /** What Search looks for; empty when not searching. */
  searchText = "";
  /** The Notes in any Folder that match `searchText`, Title matches first; empty when not searching. */
  results: SearchHit[] = [];
  /** The selected Folder's path; empty for the top level. Selecting another closes the open Note. */
  folder = "";
  /** Whether the selected Folder's Notes are pushed over the outline, where the columns don't fit. */
  showsFolder = false;
  /** The open Note's ID. */
  openId: string | undefined;
  /** The Note just added and not yet left: its editor takes the focus on the Title. */
  newId: string | undefined;
  /** Why the last save failed; none after one that worked. */
  failure: string | undefined;
  /** The Images the open Note started with or added, for release when it is left. */
  private releasable: string[] = [];
  /** Called after the rows, the open Note or the failure changed. */
  onChange: () => void = () => {};

  constructor(private readonly files: NoteFiles) {
    this.reload();
  }

  private get store() {
    return this.files.store;
  }

  /** The top level, then every Folder below its parent, one level deeper: what the outline and the Move dialog list. */
  get outline(): FolderRow[] {
    return [
      { name: "Notes", path: "", depth: 0 },
      ...this.folders.map((folder) => ({ ...folder, depth: folder.depth + 1 })),
    ];
  }

  /** Settles when what was changed is saved. */
  get saving(): Promise<void> {
    return this.files.save().catch(() => {});
  }

  /** Adds an empty Note to the selected Folder and opens it, for its Title to be typed. */
  add(): void {
    const note = this.store.create(this.folder);
    const id = note.id;
    note.free();
    this.newId = id;
    this.showsFolder = true;
    this.open(id);
  }

  /** Opens the Note `id`, or none; leaving a Note discards it if it is empty. */
  open(id: string | undefined): void {
    if (id === this.openId) return;
    this.leave();
    this.openId = id;
    this.releasable = id === undefined ? [] : imageTargets(this.note(id)?.body ?? "");
    if (id !== this.newId) this.newId = undefined;
    this.changed();
  }

  /** Selects the Folder at `path` (empty for the top level) and shows its Notes; the open Note closes if it is another. */
  select(path: string): void {
    if (path !== this.folder) this.closeNote();
    this.folder = path;
    this.showsFolder = true;
    this.changed();
  }

  /** Searches for `text`; results follow it and later changes. */
  search(text: string): void {
    this.searchText = text;
    this.reload();
    this.onChange();
  }

  /** Opens a Search result in its Folder. */
  openResult(hit: SearchHit): void {
    this.select(hit.folder);
    this.open(hit.id);
  }

  /** Leaves the selected Folder's Notes for the outline, where the columns don't fit; the Folder stays selected. */
  leaveFolder(): void {
    this.closeNote();
    this.showsFolder = false;
    this.changed();
  }

  /** Closes the open Note, discarding it if it is empty; the caller reports the change. */
  private closeNote(): void {
    this.leave();
    this.openId = this.newId = undefined;
    this.releasable = [];
  }

  /** Leaving the open Note: releases its Images that no Note shows any more, then discards it if it is empty. */
  private leave(): void {
    const id = this.openId;
    if (id === undefined) return;
    if (this.releasable.length > 0) this.store.releaseImages(id, this.releasable);
    this.store.discardIfEmpty(id);
  }

  /** Keeps a PNG, JPEG, GIF or WebP beside the Note `id`; its reference, or `undefined` for any other format. */
  addImage(id: string, bytes: Uint8Array): string | undefined {
    let reference: string;
    try {
      reference = this.store.addImage(id, bytes);
    } catch {
      return undefined;
    }
    if (id === this.openId) this.releasable.push(reference);
    this.changed();
    return reference;
  }

  /** The bytes of the Image `reference` names beside the Note `id`; `undefined` if there is none. */
  async readImage(id: string, reference: string): Promise<Uint8Array | undefined> {
    try {
      return this.store.readImage(id, reference);
    } catch {
      return undefined;
    }
  }

  /** The Note `id` as last saved. */
  note(id: string): NoteText | undefined {
    try {
      const note = this.store.read(id);
      const text = { title: note.title, body: note.body };
      note.free();
      return text;
    } catch {
      return undefined;
    }
  }

  /** Saves the Title and Body as typed. */
  save(id: string, title: string, body: string): void {
    try {
      this.store.edit(id, title, body).free();
    } catch {
      return;
    }
    this.changed();
  }

  /** Whether Deleting the Note asks first: its Body has text. */
  confirmsDelete(id: string): boolean {
    return (this.note(id)?.body ?? "") !== "";
  }

  /** Deletes the Note; the open one closes. */
  delete(id: string): void {
    if (this.openId === id) {
      this.openId = undefined;
      this.releasable = [];
    }
    this.store.delete(id);
    this.changed();
  }

  /** Makes a Folder in `parent` and selects it; throws the core's reason for a name it refuses. */
  addFolder(name: string, parent: string): void {
    const path = this.store.createFolder(parent, name);
    this.select(path);
  }

  /** Renames the Folder at `path` (throwing as `addFolder` does); a selected Folder, or one around it, stays selected. */
  renameFolder(path: string, name: string): void {
    this.relocate(path, this.store.renameFolder(path, name));
  }

  /** Moves the Folder at `path` into the Folder `parent` (empty for the top level), throwing the core's reason for a Move it refuses. */
  moveFolder(path: string, parent: string): void {
    this.relocate(path, this.store.moveFolder(path, parent));
  }

  /** Moves the Note into the Folder at `folder` (empty for the top level), throwing if it can't be. An open Note stays open. */
  moveNote(id: string, folder: string): void {
    this.store.moveNote(id, folder);
    this.changed();
  }

  /** What Deleting the Folder at `path` would Delete, for the confirmation; empty if nothing. */
  summary(path: string): string {
    const counts = this.store.count(path);
    const { notes, folders } = counts;
    counts.free();
    if (notes === 0 && folders === 0) return "It is empty.";
    const part = (n: number, one: string, many: string) => (n === 1 ? `1 ${one}` : `${n} ${many}`);
    return `This Deletes ${part(notes, "Note", "Notes")} and ${part(folders, "Folder", "Folders")}.`;
  }

  /** Deletes the Folder with everything in it; the selection moves out of it. */
  deleteFolder(path: string): void {
    this.store.deleteFolder(path);
    if (this.folder === path || this.folder.startsWith(`${path}/`)) {
      this.folder = path.slice(0, Math.max(path.lastIndexOf("/"), 0));
    }
    // A Note in the Folder is gone with it.
    if (this.openId !== undefined && !this.note(this.openId)) {
      this.openId = undefined;
      this.releasable = [];
    }
    this.changed();
  }

  /** The selection follows a Folder that is now at `to`; an open Note stays open. */
  private relocate(from: string, to: string): void {
    if (this.folder === from || this.folder.startsWith(`${from}/`)) {
      this.folder = to + this.folder.slice(from.length);
    }
    this.changed();
  }

  /** Sync or another tab wrote or deleted Note files: reads them again. */
  async syncDidChange(): Promise<void> {
    await this.files.reload();
    this.reload();
    this.onChange();
  }

  /** Saves what changed, then tells the view. */
  private changed(): void {
    this.reload();
    this.files.save().then(
      () => this.setFailure(undefined),
      (error: unknown) => this.setFailure(error instanceof Error ? error.message : String(error)),
    );
    this.onChange();
  }

  private setFailure(failure: string | undefined): void {
    if (failure === this.failure) return;
    this.failure = failure;
    this.onChange();
  }

  private reload(): void {
    const { store } = this;
    this.folders = store.folders().map((folder) => {
      const row = { name: folder.name, path: folder.path, depth: folder.depth };
      folder.free();
      return row;
    });
    this.rows = store.notes(this.folder).map((note) => {
      const row = { id: note.id, title: note.title, preview: note.preview };
      note.free();
      return row;
    });
    this.results =
      this.searchText.trim() === ""
        ? []
        : store.search(this.searchText).map((hit) => {
            const result = {
              id: hit.id,
              title: hit.title,
              folder: hit.folder,
              snippet: hit.snippet,
              matchStart: hit.matchedStart,
              matchEnd: hit.matchedEnd,
              titleMatched: hit.titleMatched,
            };
            hit.free();
            return result;
          });
  }
}
