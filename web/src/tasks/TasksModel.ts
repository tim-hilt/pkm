import { Planning, isValidTag, stripForbiddenTagCharacters } from "../core/Core";
import type { TaskFiles } from "./TaskFiles";

/** A Task as a row of the list shows it: a plain copy, as the core's objects must be freed. */
export interface ListedTask {
  id: string;
  title: string;
  /** 1 (highest) to 4. */
  priority: number;
  planning: Planning;
  /** Plain text; empty if none. */
  description: string;
  /** In the order they were added. */
  tags: string[];
  /** How deep it sits in the tree, 0 for a top-level Task. */
  depth: number;
  /** Closed as Done, the Close still pending. */
  done: boolean;
  /** A Closed Subtask of an Open Task. */
  closed: boolean;
  /** Whether any Task it's Blocked by is Open. */
  blocked: boolean;
}

/** The two lists, in the order they are offered, and what an empty one says. */
export const lists = [
  { planning: Planning.Active, title: "Active", empty: "Nothing active." },
  { planning: Planning.Backlog, title: "Backlog", empty: "The Backlog is empty." },
];

/** What the edit view changes of a Task. */
export interface TaskEdit {
  title: string;
  description: string;
  priority: number;
  tags: string[];
}

/**
 * The Tasks Context's state: the list being shown (Active or the Backlog),
 * from the core's store, which every change is saved from. Views follow
 * `onChange`. A port of the Swift `TasksModel`.
 */
export class TasksModel {
  /** Called after the list or `failure` changed. */
  onChange: () => void = () => {};
  /** Why the last save failed; `undefined` while saves go through. */
  failure: string | undefined;
  /** The Tasks planned as `planning`, in their order. */
  private shown: ListedTask[] = [];
  private list = Planning.Active;
  private saving: Promise<void> = Promise.resolve();

  constructor(private readonly files: TaskFiles) {
    this.reload();
  }

  /** The list shown. */
  get planning(): Planning {
    return this.list;
  }

  set planning(planning: Planning) {
    this.list = planning;
    this.reload();
    this.onChange();
  }

  get tasks(): readonly ListedTask[] {
    return this.shown;
  }

  /** Adds a Task and shows the list it landed in. False, adding nothing, if the Title is empty. */
  add(title: string, planning = this.list): boolean {
    if (!this.change(() => this.files.store.create(title, planning))) return false;
    this.list = planning;
    this.reload();
    this.onChange();
    return true;
  }

  /**
   * Saves a Task's Title, Description, Priority and Tags (adding and removing
   * what differs). False, saving nothing, if the Title is empty or a Tag is invalid.
   */
  edit(task: ListedTask, { title, description, priority, tags }: TaskEdit): boolean {
    if (!tags.every(isValidTag)) return false;
    const { store } = this.files;
    const saved = this.change(() => {
      store.edit(task.id, title, description, priority);
      for (const tag of task.tags) if (!tags.includes(tag)) store.removeTag(task.id, tag);
      for (const tag of tags) if (!task.tags.includes(tag)) store.addTag(task.id, tag);
    });
    if (saved) this.reload();
    this.onChange();
    return saved;
  }

  /** Makes a Task Active or moves it to the Backlog; it leaves the list shown. */
  plan(task: ListedTask, planning: Planning): void {
    this.change(() => this.files.store.plan(task.id, planning));
    this.reload();
    this.onChange();
  }

  /** The known Tags starting with `prefix`, to pick from. */
  knownTags(prefix: string): string[] {
    return this.files.store.tagsWithPrefix(prefix);
  }

  /** `text` without the characters a Tag can never contain. */
  static allowedTagText(text: string): string {
    return stripForbiddenTagCharacters(text);
  }

  /** Resolves once everything changed so far is saved (or has failed to be). */
  async saved(): Promise<void> {
    await this.saving;
  }

  /** Runs `change` on the store, then saves what it did. False if the store refused it. */
  private change(change: () => void): boolean {
    try {
      change();
    } catch {
      return false;
    } finally {
      this.save();
    }
    return true;
  }

  private save(): void {
    this.saving = this.files.save().then(
      () => {
        if (this.failure === undefined) return;
        this.failure = undefined;
        this.onChange();
      },
      (error: unknown) => {
        this.failure = error instanceof Error ? error.message : "The Tasks could not be saved.";
        this.onChange();
      },
    );
  }

  private reload(): void {
    const rows = this.files.store.tasks(this.list);
    this.shown = rows.map((row) => ({
      id: row.id,
      title: row.title,
      priority: row.priority,
      planning: row.planning,
      description: row.description,
      tags: row.tags,
      depth: row.depth,
      done: row.done,
      closed: row.closed,
      blocked: row.blocked,
    }));
    rows.forEach((row) => row.free());
  }
}
