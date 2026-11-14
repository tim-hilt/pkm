import { Closing, Planning, isValidTag, stripForbiddenTagCharacters } from "../core/Core";
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
  /** The Parent's ID, if it is a Subtask. */
  parent: string | undefined;
  /** How deep it sits in the tree, 0 for a top-level Task. */
  depth: number;
  /** Whether it has Subtasks, which follow it in the list. */
  hasSubtasks: boolean;
  /** Closed as Done, the Close still pending. */
  done: boolean;
  /** A Closed Subtask of an Open Task. */
  closed: boolean;
  /** Whether any Task it's Blocked by is Open. */
  blocked: boolean;
  /** The Tasks it's Blocked by, Open or Closed. */
  blockedBy: Blocker[];
}

/** A Task another Task is Blocked by; a `closed` one no longer blocks. */
export interface Blocker {
  id: string;
  title: string;
  closed: boolean;
}

/** The Close that can still be Reverted, for the toast. */
export interface PendingClose {
  title: string;
  closing: Closing;
}

/** The two lists, in the order they are offered, and what an empty one says. */
export const lists = [
  { planning: Planning.Active, title: "Active", empty: "Nothing active." },
  {
    planning: Planning.Backlog,
    title: "Backlog",
    empty: "The Backlog is empty.",
  },
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
 * `onChange`. A Close is pending, and can be Reverted, until its toast has
 * been shown for `revertWindow` milliseconds, the next Close, or `commit`. A
 * port of the Swift `TasksModel`.
 */
export class TasksModel {
  /** Called after the list or `failure` changed. */
  onChange: () => void = () => {};
  /** Why the last save failed; `undefined` while saves go through. */
  failure: string | undefined;
  /** Called after a change was written to storage, so that Sync follows. */
  onSaved: () => void = () => {};
  /** The Open Task whose edit view is shown, if any. */
  editing: ListedTask | undefined;
  /** The Close the toast offers to Revert. */
  pendingClose: PendingClose | undefined;
  /** The Tasks planned as `planning`, in their order, each followed by its Subtasks. */
  private shown: ListedTask[] = [];
  private list = Planning.Active;
  private saving: Promise<void> = Promise.resolve();
  private commitTimer: ReturnType<typeof setTimeout> | undefined;
  /** Parents whose Subtasks are hidden. */
  private readonly collapsed = new Set<string>();

  constructor(
    private readonly files: TaskFiles,
    private readonly revertWindow = 5000,
  ) {
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

  /** `tasks` without the Subtasks of a collapsed Parent. */
  get rows(): ListedTask[] {
    let hiddenBelow: number | undefined;
    return this.shown.filter((task) => {
      if (hiddenBelow !== undefined) {
        if (task.depth > hiddenBelow) return false;
        hiddenBelow = undefined;
      }
      if (this.collapsed.has(task.id)) hiddenBelow = task.depth;
      return true;
    });
  }

  /**
   * Shows the edit view of the Open Task `id`, over the list it is in;
   * `undefined` shows none. An unknown or Closed `id` shows the Active list.
   */
  open(id: string | undefined): void {
    this.editing = undefined;
    if (id !== undefined) {
      const isTask = (task: ListedTask) => task.id === id && !task.closed;
      const found = lists
        .map(({ planning }) => ({
          planning,
          task: this.read(planning).find(isTask),
        }))
        .find(({ task }) => task);
      this.list = found?.planning ?? Planning.Active;
      this.editing = found?.task;
    }
    this.reload();
    this.onChange();
  }

  /**
   * Sync or another tab wrote or deleted Task files: reads them again. A
   * pending Close, even one made meanwhile, is committed, as it can't outlive
   * the store.
   */
  async syncDidChange(): Promise<void> {
    this.commit();
    await this.files.reload();
    clearTimeout(this.commitTimer);
    this.pendingClose = undefined;
    this.reload();
    this.onChange();
  }

  isCollapsed(task: ListedTask): boolean {
    return this.collapsed.has(task.id);
  }

  /** Hides or shows a Parent's Subtasks. */
  toggleCollapsed(task: ListedTask): void {
    if (!this.collapsed.delete(task.id)) this.collapsed.add(task.id);
    this.onChange();
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
    return this.apply(() => {
      store.edit(task.id, title, description, priority);
      for (const tag of task.tags) if (!tags.includes(tag)) store.removeTag(task.id, tag);
      for (const tag of tags) if (!task.tags.includes(tag)) store.addTag(task.id, tag);
    });
  }

  /** Adds a Subtask under `parent` and shows it. False, adding nothing, if the Title is empty. */
  addSubtask(title: string, parent: ListedTask): boolean {
    this.collapsed.delete(parent.id);
    return this.apply(() => this.files.store.createSubtask(parent.id, title));
  }

  /** Makes a Task Active or moves it to the Backlog; it leaves the list shown. */
  plan(task: ListedTask, planning: Planning): void {
    this.apply(() => this.files.store.plan(task.id, planning));
  }

  /**
   * Moves a Task to be a sibling before `before`, or, with none, the last
   * top-level Task. It takes its new top-level Task's list. Refused moves
   * (under its own Subtask) change nothing.
   */
  moveBefore(task: ListedTask, before: ListedTask | undefined): void {
    this.move(task, before?.parent, before?.id);
  }

  /** Moves a Task to be the last Subtask of `parent`. */
  moveUnder(task: ListedTask, parent: ListedTask): void {
    this.move(task, parent.id, undefined);
  }

  private move(task: ListedTask, parent: string | undefined, before: string | undefined): void {
    this.apply(() => this.files.store.moveTo(task.id, parent, this.list, before));
  }

  /**
   * Makes a Task Blocked by exactly the Tasks `ids` (adding and removing what
   * differs). False if one is refused: itself, its Parent, its Subtask, or a loop.
   */
  setBlockedBy(task: ListedTask, ids: string[]): boolean {
    const current = task.blockedBy.map((b) => b.id);
    const { store } = this.files;
    return this.apply(() => {
      for (const id of current) if (!ids.includes(id)) store.removeBlocker(task.id, id);
      for (const id of ids) if (!current.includes(id)) store.addBlocker(task.id, id);
    });
  }

  /**
   * The Open Tasks of both lists `task` could be Blocked by: not itself, its
   * Parent or its Subtasks, and not one it is Blocked by already.
   */
  blockerCandidates(task: ListedTask): ListedTask[] {
    const blocking = new Set(task.blockedBy.map((b) => b.id));
    return [Planning.Active, Planning.Backlog]
      .flatMap((planning) => this.read(planning))
      .filter(
        (t) =>
          !t.closed &&
          !t.done &&
          t.id !== task.id &&
          t.id !== task.parent &&
          t.parent !== task.id &&
          !blocking.has(t.id),
      );
  }

  /**
   * Closes a Task, committing the previous Close first. Done leaves it in
   * place, marked, Rejected removes it; either way nothing is saved until
   * `commit`, which follows `revertWindow` later.
   */
  close(task: ListedTask, closing: Closing): void {
    clearTimeout(this.commitTimer);
    const closed = this.apply(() => {
      this.pendingClose = undefined;
      this.files.store.close(task.id, closing);
      this.pendingClose = { title: task.title, closing };
    });
    if (closed) this.commitTimer = setTimeout(() => this.commit(), this.revertWindow);
  }

  /** The circle of a Task: Done if it is Open, Reverts the Close if it is Done. */
  toggleDone(task: ListedTask): void {
    if (task.done) this.revert();
    else this.close(task, Closing.Done);
  }

  /** Takes the pending Close back, as if it never happened. */
  revert(): void {
    clearTimeout(this.commitTimer);
    this.files.store.revert();
    this.pendingClose = undefined;
    this.reload();
    this.onChange();
  }

  /** Saves the pending Close, if any; a Done Task leaves the list. */
  commit(): void {
    clearTimeout(this.commitTimer);
    if (!this.pendingClose) return;
    this.apply(() => {
      this.pendingClose = undefined;
      this.files.store.commit();
    });
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

  /** `change`, then the list and the views as it left them. False if the store refused it. */
  private apply(change: () => void): boolean {
    const done = this.change(change);
    this.reload();
    this.onChange();
    return done;
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
        this.onSaved();
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
    this.shown = this.read(this.list);
  }

  private read(planning: Planning): ListedTask[] {
    const rows = this.files.store.tasks(planning);
    const tasks = rows.map((row) => ({
      id: row.id,
      title: row.title,
      priority: row.priority,
      planning: row.planning,
      description: row.description,
      tags: row.tags,
      parent: row.parent ?? undefined,
      depth: row.depth,
      hasSubtasks: row.hasSubtasks,
      done: row.done,
      closed: row.closed,
      blocked: row.blocked,
      blockedBy: row.blockedBy().map((b) => {
        const blocker = { id: b.id, title: b.title, closed: b.closed };
        b.free();
        return blocker;
      }),
    }));
    rows.forEach((row) => row.free());
    return tasks;
  }
}
