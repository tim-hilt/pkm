import { imageTargets } from "../core/Core";
import type { Tag } from "../tasks/TasksModel";
import type { NoteFiles } from "./NoteFiles";

/** A Note as a row of the list shows it: a plain copy, as the core's objects must be freed. */
export interface ListedNote {
  id: string;
  title: string;
  /** The Body's line to show when the Title is empty. */
  preview: string;
  tags: string[];
}

/** A Note that Search found; a plain copy of the core's result. */
export interface SearchHit {
  id: string;
  title: string;
  /** Around the match in the Body, as typed. */
  snippet: string;
  /** The match in `snippet`: `matchStart` up to `matchEnd`. */
  matchStart: number;
  matchEnd: number;
  titleMatched: boolean;
}

/** A Note's Title and Body as typed, with its Tags in the order they were added. */
export interface NoteText {
  title: string;
  body: string;
  tags: string[];
}

/**
 * The Notes Context's state: the list of Notes and the open Note, from the
 * core's store, saved to storage after each change. It only forwards to the store;
 * every Notes rule lives in the core. A port of the Swift `NotesModel`.
 */
export class NotesModel {
  /** The Notes the list shows, newest changed first: those carrying `tagFilter`, or all. */
  notes: ListedNote[] = [];
  /** The Tags some Note carries, by Relevance: what the list can be narrowed to. */
  tagsInUse: string[] = [];
  /** What Search looks for; empty when not searching. */
  searchText = "";
  /** The Notes that match `searchText`, Title matches first; empty when not searching. */
  results: SearchHit[] = [];
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
  /** The Tag the user narrowed the list to, even if no Note carries it any more. */
  private chosenTag: string | undefined;

  constructor(
    private readonly files: NoteFiles,
    /** The known Tags with their Tag Colors. */
    readonly knownTags: () => Tag[] = () => [],
  ) {
    this.reload();
  }

  private get store() {
    return this.files.store;
  }

  /** The Tag the list is narrowed to; `undefined` for "All Notes", which it drops back to once no Note carries its Tag. */
  get tagFilter(): string | undefined {
    return this.chosenTag !== undefined && this.tagsInUse.includes(this.chosenTag)
      ? this.chosenTag
      : undefined;
  }

  set tagFilter(tag: string | undefined) {
    this.chosenTag = tag;
    this.changed();
  }

  /** Settles when what was changed is saved. */
  get saving(): Promise<void> {
    return this.files.save().catch(() => {});
  }

  /** Adds an empty Note, carrying the Tag the list is narrowed to, and opens it, for its Title to be typed. */
  add(): void {
    const tag = this.tagFilter;
    const note = this.store.create(tag === undefined ? [] : [tag]);
    const id = note.id;
    note.free();
    this.newId = id;
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

  /** Searches for `text`; results follow it and later changes. */
  search(text: string): void {
    this.searchText = text;
    this.reload();
    this.onChange();
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
      const text = { title: note.title, body: note.body, tags: note.tags };
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

  /** Gives the Note exactly `tags`: those it lacks are added in order, the others removed. Tags unknown become known. */
  async setTags(id: string, tags: string[]): Promise<void> {
    const current = this.note(id)?.tags ?? [];
    await this.files.refreshTags();
    try {
      for (const tag of current.filter((tag) => !tags.includes(tag))) {
        this.store.removeTag(id, tag).free();
      }
      for (const tag of tags.filter((tag) => !current.includes(tag))) {
        this.store.addTag(id, tag).free();
      }
    } catch {
      // The Note is gone, or a Tag is invalid: what was changed is kept.
    }
    this.changed();
  }

  /** Takes `tag` off every Note, as part of Delete a Tag. */
  removeTagEverywhere(tag: string): void {
    this.store.removeTagEverywhere(tag);
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

  /**
   * Sync or another tab wrote or deleted Note files: reads them again. An
   * open Note they Deleted closes.
   */
  async syncDidChange(): Promise<void> {
    await this.files.reload();
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
    this.tagsInUse = store.tagsInUse();
    const tag = this.tagFilter;
    const rows = tag === undefined ? store.notes() : store.notesTagged(tag);
    this.notes = rows.map((note) => {
      const row = { id: note.id, title: note.title, preview: note.preview, tags: note.tags };
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
