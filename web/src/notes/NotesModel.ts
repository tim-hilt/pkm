import type { NoteFiles } from "./NoteFiles";

/** A Note as a row of the list shows it: a plain copy, as the core's objects must be freed. */
export interface ListedNote {
  id: string;
  title: string;
  /** The Body's line to show when the Title is empty. */
  preview: string;
}

/** A Note's Title and Body as typed. */
export interface NoteText {
  title: string;
  body: string;
}

/**
 * The Notes Context's state: the Notes list and the open Note, from the core's
 * store, saved to storage after each change. It only forwards to the store;
 * every Notes rule lives in the core. A port of the Swift `NotesModel`.
 */
export class NotesModel {
  /** Every Note, newest changed first. */
  rows: ListedNote[] = [];
  /** The open Note's ID. */
  openId: string | undefined;
  /** The Note just added and not yet left: its editor takes the focus on the Title. */
  newId: string | undefined;
  /** Why the last save failed; none after one that worked. */
  failure: string | undefined;
  /** Called after the rows, the open Note or the failure changed. */
  onChange: () => void = () => {};

  constructor(private readonly files: NoteFiles) {
    this.reload();
  }

  /** Settles when what was changed is saved. */
  get saving(): Promise<void> {
    return this.files.save().catch(() => {});
  }

  /** Adds an empty Note and opens it, for its Title to be typed. */
  add(): void {
    const note = this.files.store.create();
    const id = note.id;
    note.free();
    this.newId = id;
    this.open(id);
  }

  /** Opens the Note `id`, or none; leaving a Note discards it if it is empty. */
  open(id: string | undefined): void {
    if (id === this.openId) return;
    if (this.openId !== undefined) this.files.store.discardIfEmpty(this.openId);
    this.openId = id;
    if (id !== this.newId) this.newId = undefined;
    this.changed();
  }

  /** The Note `id` as last saved. */
  note(id: string): NoteText | undefined {
    try {
      const note = this.files.store.read(id);
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
      this.files.store.edit(id, title, body).free();
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
    if (this.openId === id) this.openId = undefined;
    this.files.store.delete(id);
    this.changed();
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
    this.rows = this.files.store.notes().map((note) => {
      const row = { id: note.id, title: note.title, preview: note.preview };
      note.free();
      return row;
    });
  }
}
