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

/** A row of the tree: a Folder, or a Note of the Folder above it at `depth` − 1. */
export type TreeRow =
  | { kind: "folder"; name: string; path: string; depth: number; expanded: boolean }
  | ({ kind: "note"; folder: string; depth: number } & ListedNote);

/** A Note's Title and Body as typed. */
export interface NoteText {
  title: string;
  body: string;
}

/**
 * The Notes Context's state: the tree of Folders and Notes, the selected
 * Folder and the open Note, from the core's store, saved to storage after each change. It only forwards to the store;
 * every Notes rule lives in the core. A port of the Swift `NotesModel`.
 */
export class NotesModel {
  /** Every Folder, parents before their Folders. */
  folders: FolderRow[] = [];
  /** The Folders open in the tree, by path. */
  readonly expanded = new Set<string>();
  /** The top level's Folders and Notes, each open Folder followed by its own. */
  tree: TreeRow[] = [];
  /** What Search looks for; empty when not searching. */
  searchText = "";
  /** The Notes in any Folder that match `searchText`, Title matches first; empty when not searching. */
  results: SearchHit[] = [];
  /** The selected Folder's path, where new Notes and Folders go; empty for the top level. Selecting another closes the open Note. */
  folder = "";
  /** The open Note's ID. */
  openId: string | undefined;
  /** The Note just added and not yet left: its editor takes the focus on the Title. */
  newId: string | undefined;
  /** Why the last save failed; none after one that worked. */
  failure: string | undefined;
  /** How often Sync changed the files: when it grows, an open editor takes the Note as saved. */
  syncs = 0;
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

  /** The top level, then every Folder below its parent, one level deeper: where the Move dialog Moves to. */
  get outline(): FolderRow[] {
    return [
      { name: "Top Level", path: "", depth: 0 },
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
    this.expand(this.folder);
    this.open(id);
  }

  /** Opens the Note `id`, or none, selecting the Folder the tree shows it in; leaving a Note discards it if it is empty. */
  open(id: string | undefined): void {
    if (id === this.openId) return;
    const row = this.tree.find((row) => row.kind === "note" && row.id === id);
    if (row?.kind === "note") this.folder = row.folder;
    this.leave();
    this.openId = id;
    this.releasable = id === undefined ? [] : imageTargets(this.note(id)?.body ?? "");
    if (id !== this.newId) this.newId = undefined;
    this.changed();
  }

  /** Selects the Folder at `path` (empty for the top level) and opens the Folders around it; the open Note closes if it is another. */
  select(path: string): void {
    if (path !== this.folder) this.closeNote();
    this.folder = path;
    this.expand(path.slice(0, Math.max(path.lastIndexOf("/"), 0)));
    this.changed();
  }

  /** Selects the Folder at `path` and opens it in the tree, or closes it if it is open. */
  toggle(path: string): void {
    if (!this.expanded.delete(path)) this.expanded.add(path);
    this.select(path);
  }

  /** Opens the Folder at `path` in the tree, and every Folder around it. */
  private expand(path: string): void {
    let at = 0;
    while (path !== "" && at !== -1) {
      at = path.indexOf("/", at + 1);
      this.expanded.add(at === -1 ? path : path.slice(0, at));
    }
  }

  /** The Notes in the Folder at `path` (empty for the top level), newest changed first. */
  notesIn(path: string): ListedNote[] {
    return this.store.notes(path).map((note) => {
      const row = { id: note.id, title: note.title, preview: note.preview };
      note.free();
      return row;
    });
  }

  /** Searches for `text`; results follow it and later changes. */
  search(text: string): void {
    this.searchText = text;
    this.reload();
    this.onChange();
  }

  /** Opens a Search result in its Folder, which the tree opens. */
  openResult(hit: SearchHit): void {
    this.select(hit.folder);
    this.expand(hit.folder);
    this.open(hit.id);
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

  /** The Note as text to copy: its Title, then its Body after a blank line; either may be missing. */
  copyText(id: string): string {
    const { title = "", body = "" } = this.note(id) ?? {};
    return [title, body].filter((part) => part !== "").join("\n\n");
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

  /** The selection and the open Folders follow a Folder that is now at `to`; an open Note stays open. */
  private relocate(from: string, to: string): void {
    const follow = (path: string) =>
      path === from || path.startsWith(`${from}/`) ? to + path.slice(from.length) : path;
    this.folder = follow(this.folder);
    const open = [...this.expanded].map(follow);
    this.expanded.clear();
    open.forEach((path) => this.expanded.add(path));
    this.changed();
  }

  /**
   * Sync or another tab wrote or deleted Note files: reads them again. A
   * Folder they took leaves the selection for the nearest one still there,
   * and an open Note they Deleted closes; one they Moved stays open.
   */
  async syncDidChange(): Promise<void> {
    await this.files.reload();
    const held = new Set(
      this.store.folders().map((folder) => {
        const { path } = folder;
        folder.free();
        return path;
      }),
    );
    while (this.folder !== "" && !held.has(this.folder)) {
      this.folder = this.folder.slice(0, Math.max(this.folder.lastIndexOf("/"), 0));
    }
    if (this.openId !== undefined && !this.note(this.openId)) {
      this.openId = this.newId = undefined;
      this.releasable = [];
    }
    this.syncs += 1;
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
    this.tree = [];
    const list = (path: string, depth: number) => {
      for (const { name, path: inner } of this.folders) {
        if (inner.slice(0, Math.max(inner.lastIndexOf("/"), 0)) !== path) continue;
        const expanded = this.expanded.has(inner);
        this.tree.push({ kind: "folder", name, path: inner, depth, expanded });
        if (expanded) list(inner, depth + 1);
      }
      for (const note of this.notesIn(path))
        this.tree.push({ kind: "note", folder: path, depth, ...note });
    };
    list("", 0);
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
