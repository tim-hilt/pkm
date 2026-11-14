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
  /** The Tag Color (`#rrggbb`) of each of `tags` that has one; the others are Default. */
  tagColors: Record<string, string>;
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

/** What a new Task has besides its Title. */
export interface NewTask {
  /** Plain text; none if omitted. */
  description?: string;
  /** 1 (highest) to 4; 4 if omitted. */
  priority?: number;
  /** Tags to add; ones that don't exist yet are created. */
  tags?: string[];
}

/** A Tag that exists, with its Tag Color. */
export interface Tag {
  name: string;
  /** `#rrggbb`; `undefined` is Default. */
  color: string | undefined;
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
  /** The Close the toast offers to Revert. */
  pendingClose: PendingClose | undefined;
  /** The IDs of the Tasks of the edit view, the first opened first: each Blocker opened is pushed. */
  private stack: string[] = [];
  /** Whether the Task on top was last saved with an empty Title, which keeps the stack as it is. */
  private titleMissing = false;
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

  /** The Task on top of the edit view's stack as last saved, from either list; `undefined` if none is shown. */
  get editing(): ListedTask | undefined {
    const id = this.stack.at(-1);
    return id === undefined ? undefined : this.find(id);
  }

  /** How many Tasks the edit view's stack has. */
  get depth(): number {
    return this.stack.length;
  }

  /** Whether the edit view can't be left or popped: the Title on top is empty. */
  get isLeavingBlocked(): boolean {
    return this.titleMissing;
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
   * The `id` on top of the stack stays as it is, and the one below it is popped
   * to; any other starts a stack of its own.
   */
  open(id: string | undefined): void {
    if (id !== undefined && id === this.stack.at(-1)) return;
    if (id !== undefined && id === this.stack.at(-2)) return this.back();
    this.clearStack();
    if (id !== undefined) {
      const isTask = (task: ListedTask) => task.id === id && !task.closed;
      const found = lists
        .map(({ planning }) => ({
          planning,
          task: this.read(planning).find(isTask),
        }))
        .find(({ task }) => task);
      this.list = found?.planning ?? Planning.Active;
      if (found?.task) this.stack = [id];
    }
    this.reload();
    this.onChange();
  }

  /**
   * Pushes the Open Task `id`, a Blocker of the one on top, onto the stack; the
   * list shown stays. False, pushing nothing, if it is Closed or gone, or
   * while the Title on top is empty.
   */
  openBlocker(id: string): boolean {
    const blocker = this.find(id);
    if (this.stack.length === 0 || this.titleMissing || !blocker || blocker.closed) return false;
    this.stack.push(id);
    this.onChange();
    return true;
  }

  /** Pops the Task on top, to show the one below as last saved; refused while its Title is empty or it is the last. */
  back(): void {
    if (this.stack.length < 2 || this.titleMissing) return;
    this.stack.pop();
    this.onChange();
  }

  private clearStack(): void {
    this.stack = [];
    this.titleMissing = false;
  }

  /** `id` as last saved, from either list. */
  private find(id: string): ListedTask | undefined {
    return (
      this.shown.find((t) => t.id === id) ?? this.read(this.otherList).find((t) => t.id === id)
    );
  }

  private get otherList(): Planning {
    return this.list === Planning.Active ? Planning.Backlog : Planning.Active;
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

  /** Adds a Task and shows the list it landed in. False, adding nothing, if the Title is empty or a Tag invalid. */
  add(title: string, planning = Planning.Active, extras: NewTask = {}): boolean {
    const { store } = this.files;
    const { priority = 4, tags = [] } = extras;
    if (
      !this.change(() =>
        this.describe(store.create(title, planning, priority, tags), title, extras),
      )
    )
      return false;
    this.list = planning;
    this.reload();
    this.onChange();
    return true;
  }

  /**
   * Saves a Task's Title, Description, Priority and Tags (adding and removing
   * what differs from what is saved, however old `task` is). An empty Title
   * keeps the saved one and the rest is saved; an edit that changes nothing writes nothing. False, saving nothing, if a Tag is invalid.
   */
  edit(task: ListedTask, { title, description, priority, tags }: TaskEdit): boolean {
    if (!tags.every(isValidTag)) return false;
    if (task.id === this.stack.at(-1)) this.titleMissing = title.trim() === "";
    const { store } = this.files;
    const saved = this.current(task);
    const newTitle = title.trim() === "" ? saved.title : title.trim();
    const tagsSame = saved.tags.length === tags.length && tags.every((t) => saved.tags.includes(t));
    // Nothing differs: no write, which would bump `updated` and wake Sync.
    if (
      tagsSame &&
      newTitle === saved.title &&
      description.trim() === saved.description &&
      priority === saved.priority
    )
      return true;
    return this.apply(() => {
      store.edit(task.id, newTitle, description, priority);
      for (const tag of saved.tags) if (!tags.includes(tag)) store.removeTag(task.id, tag);
      for (const tag of tags) if (!saved.tags.includes(tag)) store.addTag(task.id, tag);
    });
  }

  /** Adds a Subtask under `parent` and shows it. False, adding nothing, if the Title is empty or a Tag invalid. */
  addSubtask(title: string, parent: ListedTask, extras: NewTask = {}): boolean {
    this.collapsed.delete(parent.id);
    const { priority = 4, tags = [] } = extras;
    return this.apply(() =>
      this.describe(
        this.files.store.createSubtask(parent.id, title, priority, tags),
        title,
        extras,
      ),
    );
  }

  /** Gives the Task `id`, just created from `title`, its Description. */
  private describe(id: string, title: string, { description = "", priority = 4 }: NewTask): void {
    if (description.trim() !== "") this.files.store.edit(id, title, description, priority);
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
   * differs). False if one is refused, which `blockerCandidates` never offers.
   */
  setBlockedBy(task: ListedTask, ids: string[]): boolean {
    const current = this.current(task).blockedBy.map((b) => b.id);
    if (current.length === ids.length && ids.every((id) => current.includes(id))) return true;
    const { store } = this.files;
    return this.apply(() => {
      for (const id of current) if (!ids.includes(id)) store.removeBlocker(task.id, id);
      for (const id of ids) if (!current.includes(id)) store.addBlocker(task.id, id);
    });
  }

  /** The Open Tasks of both lists `task` could be Blocked by (see the store). */
  blockerCandidates(task: ListedTask): ListedTask[] {
    return this.listed(this.files.store.blockerCandidates(task.id));
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

  /**
   * What a Tag field offers for the typed `query`, leaving out the Tags the Task
   * has: known Tags starting with it, then containing it, and whether to offer Create.
   */
  suggestTags(query: string, leaveOut: string[]): { matches: string[]; create: boolean } {
    const found = this.files.store.suggestTags(query, leaveOut);
    const plain = { matches: found.matches, create: found.create };
    found.free();
    return plain;
  }

  /** Every known Tag with its Tag Color, alphabetically. */
  get allTags(): Tag[] {
    return this.files.store.allTags().map((tag) => {
      const plain = { name: tag.name, color: tag.color ?? undefined };
      tag.free();
      return plain;
    });
  }

  /** Gives a Tag its Tag Color (`#rrggbb`), or Default for `undefined`; open lists show it at once. */
  setTagColor(tag: string, color: string | undefined): void {
    this.apply(() => this.files.store.setTagColor(tag, color));
  }

  /** Deletes a Tag: Removes it from every Task, Open or Closed, and forgets it and its Tag Color. */
  deleteTag(tag: string): void {
    this.apply(() => this.files.store.deleteTag(tag));
  }

  /** `text` without the characters a Tag can never contain. */
  static allowedTagText(text: string): string {
    return stripForbiddenTagCharacters(text);
  }

  /** Resolves once everything changed so far is saved (or has failed to be). */
  async saved(): Promise<void> {
    await this.saving;
  }

  /** `task` as last saved: the list is read after every change, a `task` held meanwhile is stale. */
  private current(task: ListedTask): ListedTask {
    return this.find(task.id) ?? task;
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
    return this.listed(this.files.store.tasks(planning));
  }

  /** The wasm `rows` as plain Tasks, freeing them. */
  private listed(rows: ReturnType<TaskFiles["store"]["tasks"]>): ListedTask[] {
    const tasks = rows.map((row) => {
      const tags = row.tags();
      const task = {
        id: row.id,
        title: row.title,
        priority: row.priority,
        planning: row.planning,
        description: row.description,
        tags: tags.map((tag) => tag.name),
        tagColors: Object.fromEntries(
          tags.filter((tag) => tag.color).map((tag) => [tag.name, tag.color!]),
        ),
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
      };
      tags.forEach((tag) => tag.free());
      return task;
    });
    rows.forEach((row) => row.free());
    return tasks;
  }
}
