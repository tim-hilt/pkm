import {
  Closing,
  type Direction,
  type FilterSortKey,
  TaskQuickAdd,
  isValidTag,
  stripForbiddenTagCharacters,
  typedQuery,
  unfinishedQueryTag,
} from "../core/Core";
import type { TaskFiles } from "./TaskFiles";

/** A Task as a row of the list shows it: a plain copy, as the core's objects must be freed. */
export interface ListedTask {
  id: string;
  title: string;
  /** 1 (highest) to 4. */
  priority: number;
  /** Plain text; empty if none. */
  description: string;
  /** In the order they were added. */
  tags: string[];
  /** The Tag Color (`#rrggbb`) of each of `tags` that has one; the others are Default. */
  tagColors: Record<string, string>;
  /** The Parent's ID, if it is a Subtask. */
  parent: string | undefined;
  /** The Parent's Title, if it is a Subtask. */
  parentTitle: string | undefined;
  /** How many of its direct Subtasks are still Open. */
  openSubtasks: number;
  /** Closed as Done, the Close still pending. */
  done: boolean;
  /** Whether any Task it's Blocked by is Open. */
  blocked: boolean;
  /** The Tasks it's Blocked by, Open or Closed. */
  blockedBy: Blocker[];
}

/** The Tasks a Filter shows under one heading. */
export interface TaskGroup {
  /** The Group-by Tag heading it; `undefined` for the Tasks without a Group, which have no heading. */
  tag: string | undefined;
  /** The Tag Color (`#rrggbb`) of `tag`; `undefined` is Default. */
  color: string | undefined;
  tasks: ListedTask[];
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

/** What the Tasks Context shows: the Filters list, or the Open Tasks a Filter matches. */
export type TasksList = { kind: "filters" } | { kind: "filter"; id: string };

/** Whether `a` and `b` show the same list. */
function sameList(a: TasksList, b: TasksList): boolean {
  return storedList(a) === storedList(b);
}

/** The list as a Device stores it: `filters` or `filter:<id>` (as on Apple). */
function storedList(list: TasksList): string {
  return list.kind === "filters" ? "filters" : `filter:${list.id}`;
}

function parseList(stored: string | null): TasksList | undefined {
  if (stored === "filters") return { kind: "filters" };
  if (stored?.startsWith("filter:")) return { kind: "filter", id: stored.slice("filter:".length) };
  return undefined;
}

/** What a Device keeps: the part of `localStorage` the last list uses. */
export interface DeviceStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const lastListKey = "lastTasksList";

/** How a Filter orders its Tasks: by a key, which has a Direction unless it is Manual. */
export interface Sort {
  key: FilterSortKey;
  direction: Direction | undefined;
}

/** A Filter: a Title and a Query over the Open Tasks' Tags. */
export interface Filter {
  id: string;
  title: string;
  /** As the user typed it. */
  query: string;
  favorite: boolean;
  sort: Sort;
  /** The Tags its Tasks are split by, as typed: `@work @private`; empty if it isn't Grouped. */
  groupBy: string;
  /** How many Open Tasks the Query matches. */
  matches: number;
  /** Why the stored Query can't be read; such a Filter matches nothing until it is edited. */
  error: string | undefined;
}

/** What checking a Query text found, without saving it. */
export interface QueryCheck {
  /** Why it can't be read, and where (in characters from its start). */
  problem: { position: number; message: string } | undefined;
  /** The Tags it names that don't exist: allowed, but worth marking. */
  unknown: string[];
}

/** What the edit view changes of a Task. */
export interface TaskEdit {
  title: string;
  description: string;
  priority: number;
  tags: string[];
}

/**
 * The Tasks Context's state: the list being shown (the Filters or a Filter's Tasks),
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
  /** The IDs of the Tasks of the edit view, the first opened first: each Blocker or Subtask opened is pushed. */
  private stack: string[] = [];
  /** Whether the Task on top was last saved with an empty Title, which keeps the stack as it is. */
  private titleMissing = false;
  /** The Groups of the list shown, in order; none for the Filters list. */
  private shownGroups: TaskGroup[] = [];
  /** The Tasks of `shownGroups`, one after another. */
  private shown: ListedTask[] = [];
  private shownList: TasksList = { kind: "filters" };
  private all: Filter[] = [];
  private saving: Promise<void> = Promise.resolve();
  private commitTimer: ReturnType<typeof setTimeout> | undefined;

  constructor(
    private readonly files: TaskFiles,
    private readonly revertWindow = 5000,
    private readonly device?: DeviceStorage,
  ) {
    this.reload();
    // The last list opens again, unless it is a Filter that is gone: then the start list.
    const last = parseList(device?.getItem(lastListKey) ?? null);
    const usable = last && (last.kind !== "filter" || this.all.some((f) => f.id === last.id));
    this.shownList = usable ? last : this.startList;
    this.reload();
  }

  /** The Tasks bar's entries: the Favorites in the Filters list's order, Filters. */
  get bar(): { title: string; list: TasksList }[] {
    return [
      ...this.all
        .filter((f) => f.favorite)
        .map((f) => ({
          title: f.title,
          list: { kind: "filter" as const, id: f.id },
        })),
      { title: "Filters", list: { kind: "filters" as const } },
    ];
  }

  /** Where a Device with no usable last list starts: the first Favorite, or the Filters list. */
  private get startList(): TasksList {
    const favorite = this.all.find((f) => f.favorite);
    return favorite ? { kind: "filter", id: favorite.id } : { kind: "filters" };
  }

  /** The bar's entry that shows the list shown: a Filter that isn't a Favorite is under Filters. */
  get barEntry(): TasksList {
    const entry = this.bar.find(({ list }) => sameList(list, this.shownList));
    return entry?.list ?? { kind: "filters" };
  }

  /** The list shown. */
  get list(): TasksList {
    return this.shownList;
  }

  set list(list: TasksList) {
    this.show(list);
    this.reload();
    this.onChange();
  }

  /** Makes `list` the one shown, which this Device opens on again; read it with `reload`. */
  private show(list: TasksList): void {
    this.shownList = list;
    this.device?.setItem(lastListKey, storedList(list));
  }

  /** Every Filter, in the Filters list's order. */
  get filters(): readonly Filter[] {
    return this.all;
  }

  /** The Filter whose Tasks are shown. */
  get filter(): Filter | undefined {
    const list = this.shownList;
    return list.kind === "filter" ? this.all.find((f) => f.id === list.id) : undefined;
  }

  /** The Task on top of the edit view's stack as last saved; `undefined` if none is shown. */
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

  /** The Tasks of the list shown, in their order, one per Group they are in; none for the Filters list. */
  get tasks(): readonly ListedTask[] {
    return this.shown;
  }

  /**
   * The Groups of the Filter shown: the Tasks without a Group first, then one
   * per Group-by Tag, empty or not. A Filter without Group by is one Group.
   * None for the Filters list.
   */
  get groups(): readonly TaskGroup[] {
    return this.shownGroups;
  }

  /** Whether the Filter shown is Grouped. */
  get isGrouped(): boolean {
    return this.shownGroups.length > 1;
  }

  /**
   * Shows the edit view of the Open Task `id`, over the list shown if it holds
   * the Task, else over the first Favorite that does; `undefined` shows none.
   * An unknown or Closed `id` shows the list a Device opens on without a last one.
   * The `id` on top of the stack stays as it is, and the one below it is popped
   * to; any other starts a stack of its own.
   */
  openLinked(id: string | undefined): void {
    if (id !== undefined && id === this.stack.at(-1)) return;
    if (id !== undefined && id === this.stack.at(-2)) return this.back();
    this.clearStack();
    if (id !== undefined) {
      const isTask = (task: ListedTask) => task.id === id;
      if (!this.find(id)) {
        this.show(this.startList);
      } else {
        const favorites = this.all
          .filter((f) => f.favorite)
          .map((f): TasksList => ({ kind: "filter", id: f.id }));
        const home = [this.shownList, ...favorites].find((list) =>
          this.read(list).some((group) => group.tasks.some(isTask)),
        );
        if (home) this.show(home);
        this.stack = [id];
      }
    }
    this.reload();
    this.onChange();
  }

  /**
   * Pushes the Open Task `id`, a Blocker, Subtask or Parent of the one on top,
   * onto the stack; the list shown stays. The Task just below is popped to
   * instead. False, changing nothing, if it is Closed or gone, or while the
   * Title on top is empty.
   */
  open(id: string): boolean {
    if (this.stack.length === 0 || this.titleMissing || !this.find(id)) return false;
    if (id === this.stack.at(-2)) this.stack.pop();
    else this.stack.push(id);
    this.onChange();
    return true;
  }

  /**
   * The direct Subtasks of the Open Task `id`, in Subtask order: Open, or Done and
   * still Revertable; never Rejected.
   */
  subtasks(id: string): ListedTask[] {
    try {
      return this.listed(this.files.store.subtasks(id));
    } catch {
      return [];
    }
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

  /** The Open Task `id` as last saved, whichever list shows it. */
  find(id: string): ListedTask | undefined {
    const isTask = (task: ListedTask) => task.id === id;
    return this.shown.find(isTask) ?? this.openTasks().find(isTask);
  }

  /** Every Open Task, Subtasks included. */
  private openTasks(): ListedTask[] {
    const { store } = this.files;
    return this.listed(store.tasks());
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

  /** Quick Add for a new Task. In a Filter it starts with the Tags the Query requires, typed as Shortcuts, so that the Task stays there unless the user changes them. The caller frees it. */
  newQuickAdd(): TaskQuickAdd {
    const quickAdd = new TaskQuickAdd();
    const list = this.shownList;
    const tags = list.kind === "filter" ? this.files.store.requiredTags(list.id) : [];
    if (tags.length > 0) quickAdd.replace(0, 0, tags.map((t) => `@${t} `).join(""));
    return quickAdd;
  }

  /** Adds a Task. False, adding nothing, if the Title is empty or a Tag invalid. */
  add(title: string, extras: NewTask = {}): boolean {
    const { store } = this.files;
    const { description, priority = 4, tags = [] } = extras;
    const create = () => store.create(title, priority, tags);
    if (!this.change(() => this.describe(create(), title, priority, description))) return false;
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
      const added = tags.filter((t) => !saved.tags.includes(t));
      store.edit(task.id, newTitle, description, priority, added);
      for (const tag of saved.tags) if (!tags.includes(tag)) store.removeTag(task.id, tag);
    });
  }

  /**
   * Commits the Shortcuts of Quick Add, the Title field of the edit view, to a
   * Task: its Title without them, a Priority Shortcut replacing `priority`, its
   * Tags added to `tags` (never removed), saved like `edit`. Returns the Task as
   * saved, or `undefined`, saving nothing, if only Shortcuts were typed or the
   * save failed.
   */
  commitQuickAdd(
    quickAdd: TaskQuickAdd,
    task: ListedTask,
    { description, priority, tags }: Omit<TaskEdit, "title">,
  ): ListedTask | undefined {
    const title = quickAdd.title();
    if (title === "") return undefined;
    const merged = [...tags, ...quickAdd.tags().filter((t) => !tags.includes(t))];
    const edit = {
      title,
      description,
      priority: quickAdd.priority() ?? priority,
      tags: merged,
    };
    return this.edit(task, edit) ? this.find(task.id) : undefined;
  }

  /** Adds a Subtask under `parent` and shows it. False, adding nothing, if the Title is empty or a Tag invalid. */
  addSubtask(title: string, parent: ListedTask, extras: NewTask = {}): boolean {
    const { description, priority = 4, tags = [] } = extras;
    const create = () => this.files.store.createSubtask(parent.id, title, priority, tags);
    return this.apply(() => this.describe(create(), title, priority, description));
  }

  /** Gives the Task `id`, just created with `title` and `priority`, its Description, if any. */
  private describe(id: string, title: string, priority: number, description = ""): void {
    if (description.trim() !== "") this.files.store.edit(id, title, description, priority, []);
  }

  /**
   * Moves a Task before `before` in the Filter shown, or, with none, to its end.
   * Its Parent stays. A Subtask passing a sibling also moves in its Subtask
   * order; otherwise only the Filter's own order changes. In a Sorted Filter, a
   * drop out of the Task's Priority group turns it Manual.
   */
  moveBefore(task: ListedTask, before: ListedTask | undefined): void {
    const list = this.shownList;
    if (list.kind !== "filter") return;
    this.apply(() => this.files.store.moveInFilter(list.id, undefined, task.id, before?.id));
  }

  /**
   * Sorts a Filter by `key`, in `direction` if the key has one (else its
   * default). Manual keeps the order shown, Priority from Manual discards it.
   */
  setSort(filter: Filter, key: FilterSortKey, direction?: Direction): void {
    this.apply(() => this.files.store.setSort(filter.id, key, direction));
  }

  /** Creates a Filter at the end of the Filters list. False, creating nothing, if the Title is empty or the Query can't be read. */
  addFilter(title: string, query: string, groupBy = ""): boolean {
    return this.apply(() => this.files.store.createFilter(title, query, groupBy).free());
  }

  /** Changes a Filter's Title, Query and Group by (`@work @private`). False, changing nothing, if `addFilter` would refuse them. */
  editFilter(id: string, title: string, query: string, groupBy: string): boolean {
    return this.apply(() => this.files.store.editFilter(id, title, query, groupBy).free());
  }

  /** Deletes a Filter; no Task changes. Shown, the Filters list shows instead. */
  deleteFilter(filter: Filter): void {
    this.apply(() => this.files.store.deleteFilter(filter.id));
  }

  /** Marks a Filter as a Favorite, which gives it an entry in the bar, or unmarks it. */
  setFavorite(filter: Filter, favorite: boolean): void {
    this.apply(() => this.files.store.setFavorite(filter.id, favorite));
  }

  /** Moves a Filter before `before` in the Filters list, or, with none, to its end. */
  moveFilter(filter: Filter, before: Filter | undefined): void {
    this.apply(() => this.files.store.moveFilter(filter.id, before?.id));
  }

  /** Why the Group-by `text` (`@work @private`) can't be saved; `undefined` if it can. Saves nothing. */
  checkGroupBy(text: string): string | undefined {
    return this.files.store.checkGroupBy(text) ?? undefined;
  }

  /** What is wrong with the Query `text`, or which Tags it names that don't exist; saves nothing. */
  checkQuery(text: string): QueryCheck {
    const found = this.files.store.checkQuery(text);
    const problem = found.problem && {
      position: found.problem.position,
      message: found.problem.message,
    };
    const unknown = found.unknown;
    found.problem?.free();
    found.free();
    return { problem, unknown };
  }

  /** `text` as a Query may be typed: lowercase, without characters neither a Tag nor the Query hold. */
  static allowedQueryText(text: string): string {
    return typedQuery(text);
  }

  /** The known Tags to suggest for the Tag being typed at the end of the Query `text`, as Quick Add does; none if it doesn't end in one. */
  suggestQueryTags(text: string): string[] {
    const word = unfinishedQueryTag(text);
    return word === undefined ? [] : this.suggestTags(word, []).matches;
  }

  /** The known Tags to suggest for the Tag being typed at the end of the Group-by `text`, leaving out those already listed; none if it doesn't end in one. */
  suggestGroupByTags(text: string): string[] {
    const word = unfinishedQueryTag(text);
    if (word === undefined) return [];
    const listed = text
      .slice(0, text.length - word.length - 1)
      .split(/\s+/)
      .map((tag) => tag.replace(/^@+/, ""))
      .filter((tag) => tag !== "");
    return this.suggestTags(word, listed).matches;
  }

  /** The Query `text` with the Tag being typed at its end completed to `tag`. */
  static completingQuery(text: string, tag: string): string {
    const word = unfinishedQueryTag(text) ?? "";
    return text.slice(0, text.length - word.length) + tag;
  }

  /**
   * Moves a Subtask before its sibling `before`, or, with none, after its last
   * sibling. Its Parent stays. False if refused: `before` is no sibling.
   */
  moveSubtask(task: ListedTask, before: ListedTask | undefined): boolean {
    return this.apply(() => this.files.store.moveSubtask(task.id, before?.id));
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

  /** The Open Tasks `task` could be Blocked by (see the store). */
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

  /** Reads the list shown and the Filters again; a Filter that is gone shows the Filters list. */
  private reload(): void {
    const { store } = this.files;
    this.all = store.filters().map((row) => {
      const filter = {
        id: row.id,
        title: row.title,
        query: row.query,
        favorite: row.favorite,
        sort: { key: row.sortKey, direction: row.sortDirection ?? undefined },
        groupBy: row.groupBy,
        matches: row.matches,
        error: row.error ?? undefined,
      };
      row.free();
      return filter;
    });
    const list = this.shownList;
    if (list.kind === "filter" && !this.all.some((f) => f.id === list.id)) {
      this.shownList = { kind: "filters" };
    }
    this.shownGroups = this.read(this.shownList);
    this.shown = this.shownGroups.flatMap((group) => group.tasks);
  }

  private read(list: TasksList): TaskGroup[] {
    const { store } = this.files;
    if (list.kind !== "filter") return [];
    const colors = new Map(this.allTags.map((tag) => [tag.name, tag.color]));
    return store.filterGroups(list.id).map((group) => {
      const tag = group.tag ?? undefined;
      const rows = group.takeTasks();
      group.free();
      return {
        tag,
        color: tag === undefined ? undefined : colors.get(tag),
        tasks: this.listed(rows),
      };
    });
  }

  /** The wasm `rows` as plain Tasks, freeing them. */
  private listed(rows: ReturnType<TaskFiles["store"]["tasks"]>): ListedTask[] {
    const tasks = rows.map((row) => {
      const tags = row.tags();
      const task = {
        id: row.id,
        title: row.title,
        priority: row.priority,
        description: row.description,
        tags: tags.map((tag) => tag.name),
        tagColors: Object.fromEntries(
          tags.filter((tag) => tag.color).map((tag) => [tag.name, tag.color!]),
        ),
        parent: row.parent ?? undefined,
        parentTitle: row.parentTitle ?? undefined,
        openSubtasks: row.openSubtasks,
        done: row.done,
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
