import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import {
  type Core,
  Closing,
  Direction,
  FilterSortKey,
  TaskQuickAdd,
  loadCore,
  sortKeys,
} from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "./TaskFiles";
import { type ListedTask, TasksModel } from "./TasksModel";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

let storage: MemoryStorage;
let model: TasksModel;
beforeEach(async () => {
  storage = new MemoryStorage();
  const files = await TaskFiles.open(storage);
  for (const [title, query] of [
    ["All Tasks", "*"],
    ["Active", "!@backlog"],
    ["Backlog", "@backlog"],
  ]) {
    files.store.setFavorite(files.store.createFilter(title, query, "").id, true);
  }
  await files.save();
  model = new TasksModel(files);
});

/** Starts a Filters test from none: the others start with the Favorites All Tasks, Active and Backlog. */
const deleteFavorites = () => [...model.filters].forEach((filter) => model.deleteFilter(filter));

const titles = () => model.tasks.map((task) => task.title);
const first = () => model.tasks[0];

describe("the Tasks list", () => {
  it("starts on the first Favorite, empty", () => {
    expect(model.list).toEqual({ kind: "filter", id: model.filters[0].id });
    expect(model.tasks).toEqual([]);
  });

  it("adds a Task with Priority 4 and no Tags", () => {
    expect(model.add("Buy milk")).toBe(true);
    expect(first()).toMatchObject({
      title: "Buy milk",
      priority: 4,
      description: "",
      tags: [],
    });
  });

  it("starts a new Task in a Filter with the Tags its Query requires as Shortcuts", () => {
    model.list = { kind: "filter", id: model.filters[2].id }; // @backlog
    const quickAdd = model.newQuickAdd();
    expect(quickAdd.text()).toBe("@backlog ");
    expect(quickAdd.tags()).toEqual(["backlog"]);
    quickAdd.free();
    model.list = { kind: "filter", id: model.filters[0].id }; // *
    const plain = model.newQuickAdd();
    expect(plain.text()).toBe("");
    plain.free();
  });

  it("adds a Task with a Description", () => {
    expect(model.add("Buy milk", { description: "Two litres" })).toBe(true);
    expect(first()).toMatchObject({
      title: "Buy milk",
      description: "Two litres",
    });
  });

  it("adds a Subtask with a Description", () => {
    model.add("Shop");
    expect(model.addSubtask("Milk", first(), { description: "Oat" })).toBe(true);
    expect(model.tasks[1]).toMatchObject({ title: "Milk", description: "Oat" });
  });

  it("adds a Task with the Title, Priority and Tags of Quick Add", () => {
    const quickAdd = new TaskQuickAdd();
    quickAdd.replace(0, 0, "Fix login p1 @bug @work/x ");
    const { title, tags } = { title: quickAdd.title(), tags: quickAdd.tags() };
    expect(model.add(title, { priority: quickAdd.priority(), tags })).toBe(true);
    quickAdd.free();
    expect(first()).toMatchObject({
      title: "Fix login",
      priority: 1,
      tags: ["bug", "work/x"],
    });
    expect(model.allTags.map((t) => t.name)).toEqual(["bug", "work/x"]);
  });

  it("saves a Description of Markdown unchanged", () => {
    const markdown = "# Plan\n\n- *one*\n  2. [two](https://example.com)\n```\nx **y**\n```";
    expect(model.add("Plan", { description: markdown })).toBe(true);
    expect(first().description).toBe(markdown);
  });

  it("adds a Subtask with the Priority and Tags of Quick Add, and a Description", () => {
    model.add("Shop");
    const quickAdd = new TaskQuickAdd();
    quickAdd.replace(0, 0, "p2 Milk @food ");
    const extras = {
      description: "Oat",
      priority: quickAdd.priority(),
      tags: quickAdd.tags(),
    };
    expect(model.addSubtask(quickAdd.title(), first(), extras)).toBe(true);
    quickAdd.free();
    expect(model.tasks[1]).toMatchObject({
      title: "Milk",
      description: "Oat",
      priority: 2,
      tags: ["food"],
    });
  });

  it("refuses a Title of only Shortcuts like an empty one", () => {
    const quickAdd = new TaskQuickAdd();
    quickAdd.replace(0, 0, "p1 @bug");
    expect(model.add(quickAdd.title(), { tags: quickAdd.tags() })).toBe(false);
    quickAdd.free();
    expect(model.tasks).toEqual([]);
  });

  it("refuses an empty Title and adds nothing", () => {
    expect(model.add("   ")).toBe(false);
    expect(model.tasks).toEqual([]);
  });

  it("edits Title, Description, Priority and Tags", () => {
    model.add("Buy milk");
    const task = first();
    const edited = model.edit(task, {
      title: "Buy oat milk",
      description: "Two litres",
      priority: 1,
      tags: ["home", "shop/food"],
    });
    expect(edited).toBe(true);
    expect(first()).toMatchObject({
      title: "Buy oat milk",
      description: "Two litres",
      priority: 1,
      tags: ["home", "shop/food"],
    });
  });

  it("starts Quick Add from a saved Title with the Shortcut-like parts Ignored", () => {
    model.add("Call p1 @home");
    const quickAdd = TaskQuickAdd.fromTitle(first().title);
    expect(quickAdd.priority()).toBeUndefined();
    expect(quickAdd.tags()).toEqual([]);
    expect(quickAdd.title()).toBe(first().title);
    quickAdd.free();
  });

  it("commits Quick Add: Title without Shortcuts, Priority replaced, Tags added", () => {
    model.add("Buy milk");
    const quickAdd = TaskQuickAdd.fromTitle("Buy milk");
    quickAdd.replace(9, 9, " p1 @bug ");
    const saved = model.commitQuickAdd(quickAdd, first(), {
      description: "d",
      priority: 3,
      tags: ["home"],
    });
    quickAdd.free();
    expect(saved).toMatchObject({
      title: "Buy milk",
      description: "d",
      priority: 1,
      tags: ["home", "bug"],
    });
    expect(first()).toMatchObject(saved!);
  });

  it("commits with the Priority as it is when no Priority Shortcut is pending", () => {
    model.add("Buy milk");
    const quickAdd = TaskQuickAdd.fromTitle("Buy milk");
    quickAdd.replace(9, 9, " @bug ");
    const saved = model.commitQuickAdd(quickAdd, first(), {
      description: "",
      priority: 3,
      tags: [],
    });
    quickAdd.free();
    expect(saved).toMatchObject({ priority: 3, tags: ["bug"] });
  });

  it("lets the Priority control override a pending Priority Shortcut", () => {
    model.add("Buy milk");
    const quickAdd = TaskQuickAdd.fromTitle("Buy milk");
    quickAdd.replace(9, 9, " p1 ");
    quickAdd.ignorePriority();
    const saved = model.commitQuickAdd(quickAdd, first(), {
      description: "",
      priority: 2,
      tags: [],
    });
    quickAdd.free();
    expect(saved).toMatchObject({ title: "Buy milk p1", priority: 2 });
  });

  it("commits nothing if only Shortcuts are typed", () => {
    model.add("Buy milk");
    const quickAdd = TaskQuickAdd.fromTitle("");
    quickAdd.replace(0, 0, "p1 @bug");
    const saved = model.commitQuickAdd(quickAdd, first(), {
      description: "",
      priority: 3,
      tags: [],
    });
    quickAdd.free();
    expect(saved).toBeUndefined();
    expect(first()).toMatchObject({ title: "Buy milk", priority: 4, tags: [] });
  });

  it("removes the Tags an edit leaves out", () => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["a", "b"],
    });
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["b"],
    });
    expect(first().tags).toEqual(["b"]);
  });

  it("keeps the old Title and saves the rest if the Title is empty", () => {
    model.add("Buy milk");
    expect(
      model.edit(first(), {
        title: " ",
        description: "x",
        priority: 1,
        tags: ["a"],
      }),
    ).toBe(true);
    expect(first()).toMatchObject({
      title: "Buy milk",
      description: "x",
      priority: 1,
      tags: ["a"],
    });
  });

  it("saves nothing if a Tag is invalid", () => {
    model.add("Buy milk");
    expect(
      model.edit(first(), {
        title: "New",
        description: "x",
        priority: 1,
        tags: ["No!"],
      }),
    ).toBe(false);
    expect(first()).toMatchObject({
      title: "Buy milk",
      description: "",
      priority: 4,
    });
  });

  it("edits again from a stale Task, as an autosaving view does", () => {
    model.add("Buy milk");
    const stale = first();
    const edit = (tags: string[]) =>
      model.edit(stale, {
        title: "Buy milk",
        description: "",
        priority: 4,
        tags,
      });
    edit(["a"]);
    edit([]);
    expect(first().tags).toEqual([]);
  });

  it("suggests the known Tags that start with what is typed, then those containing it", () => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["home", "work"],
    });
    expect(model.suggestTags("h", [])).toEqual({
      matches: ["home"],
      create: true,
    });
    expect(model.suggestTags("", [])).toEqual({
      matches: ["home", "work"],
      create: false,
    });
    expect(model.suggestTags("o", [])).toEqual({
      matches: ["home", "work"],
      create: true,
    });
    expect(model.suggestTags("o", ["home"]).matches).toEqual(["work"]);
    expect(model.suggestTags("x", [])).toEqual({ matches: [], create: true });
    expect(model.suggestTags("X", []).create).toBe(false);
  });

  it("refuses the characters a Tag can't hold as they are typed", () => {
    expect(TasksModel.allowedTagText("Shop & go/2")).toBe("hopgo/2");
  });

  it("tells the views whenever the list changed", () => {
    let changes = 0;
    model.onChange = () => changes++;
    model.add("a");
    model.list = { kind: "filters" };
    model.list = { kind: "filter", id: model.filters[0].id };
    expect(changes).toBe(3);
  });

  it("saves every change to storage", async () => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 2,
      tags: ["home"],
    });
    await model.saved();
    const reopened = new TasksModel(await TaskFiles.open(storage));
    expect(reopened.tasks[0]).toMatchObject({
      title: "Buy milk",
      priority: 2,
      tags: ["home"],
    });
  });

  it("reports a failed save", async () => {
    storage.write = () => Promise.reject(new Error("full"));
    model.add("Buy milk");
    await model.saved();
    expect(model.failure).toBeDefined();
  });
});

describe("Done, Reject and Revert", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  const reopened = async () => new TasksModel(await TaskFiles.open(storage));

  it("keeps a Done Task in place, marked, with a toast", () => {
    model.add("a");
    model.close(first(), Closing.Done);
    expect(first()).toMatchObject({ title: "a", done: true });
    expect(model.pendingClose).toEqual({ title: "a", closing: Closing.Done });
  });

  it("removes a Rejected Task at once", () => {
    model.add("a");
    model.close(first(), Closing.Rejected);
    expect(titles()).toEqual([]);
    expect(model.pendingClose).toEqual({
      title: "a",
      closing: Closing.Rejected,
    });
  });

  it("lets a Done Task leave once the toast goes away, and saves it", async () => {
    model.add("a");
    model.close(first(), Closing.Done);
    vi.advanceTimersByTime(5000);
    expect(titles()).toEqual([]);
    expect(model.pendingClose).toBeUndefined();
    vi.useRealTimers();
    await model.saved();
    expect((await reopened()).tasks).toEqual([]);
  });

  it("Reverts a Close, leaving no trace", async () => {
    model.add("a");
    model.close(first(), Closing.Rejected);
    model.revert();
    expect(first()).toMatchObject({ title: "a", done: false });
    expect(model.pendingClose).toBeUndefined();
    vi.advanceTimersByTime(10000);
    vi.useRealTimers();
    await model.saved();
    expect((await reopened()).tasks).toHaveLength(1);
  });

  it("toggles a Done Task back with its circle", () => {
    model.add("a");
    model.toggleDone(first());
    expect(first().done).toBe(true);
    model.toggleDone(first());
    expect(first().done).toBe(false);
    expect(model.pendingClose).toBeUndefined();
  });

  it("makes the previous Close final at once on a new one", () => {
    model.add("a");
    model.add("b");
    model.close(model.tasks[0], Closing.Rejected);
    model.close(model.tasks[0], Closing.Rejected);
    expect(titles()).toEqual([]);
    expect(model.pendingClose?.title).toBe("b");
  });

  it("commits a pending Close when asked, as when the page is hidden", async () => {
    model.add("a");
    model.close(first(), Closing.Done);
    model.commit();
    expect(titles()).toEqual([]);
    await model.saved();
    expect((await reopened()).tasks).toEqual([]);
  });

  it("tells the views when the toast changes", () => {
    model.add("a");
    let changes = 0;
    model.onChange = () => changes++;
    model.close(first(), Closing.Done);
    vi.advanceTimersByTime(5000);
    expect(changes).toBe(2);
  });
});

describe("Subtasks", () => {
  beforeEach(() => {
    model.add("parent");
    model.addSubtask("child", first());
  });

  it("lists a Subtask after its Parent, with the markers", () => {
    expect(model.tasks.map((t) => [t.title, t.parentTitle, t.openSubtasks])).toEqual([
      ["parent", undefined, 1],
      ["child", "parent", 0],
    ]);
  });

  it("shows both markers on a Task that is Parent and Subtask, and counts only Open Subtasks", () => {
    model.addSubtask("grandchild", model.tasks[1]);
    model.close(model.tasks[2], Closing.Done);
    model.commit();
    expect(model.tasks.map((t) => t.title)).toEqual(["parent", "child"]);
    expect(model.tasks[1]).toMatchObject({
      parentTitle: "parent",
      openSubtasks: 0,
    });
  });

  it("refuses an empty Subtask Title", () => {
    expect(model.addSubtask(" ", first())).toBe(false);
  });

  it("Closes the Subtasks with a Parent, and Reverts them together", () => {
    model.close(first(), Closing.Rejected);
    expect(titles()).toEqual([]);
    model.revert();
    expect(titles()).toEqual(["parent", "child"]);
  });

  it("drops a Closed Subtask from the list once the Close is committed", () => {
    model.close(model.tasks[1], Closing.Done);
    expect(model.tasks.map((t) => [t.title, t.done])).toEqual([
      ["parent", false],
      ["child", true],
    ]);
    model.commit();
    expect(titles()).toEqual(["parent"]);
  });
});

describe("order and moving", () => {
  beforeEach(() => {
    for (const title of ["a", "b", "c"]) model.add(title);
  });
  const task = (title: string) => model.tasks.find((t) => t.title === title)!;

  it("moves a Task before a sibling", () => {
    model.moveBefore(task("c"), task("a"));
    expect(titles()).toEqual(["c", "a", "b"]);
  });

  it("moves a Task to the end of the top level", () => {
    model.moveBefore(task("a"), undefined);
    expect(titles()).toEqual(["b", "c", "a"]);
  });

  it("keeps a Subtask's Parent when it moves above it", () => {
    model.addSubtask("sub", task("a"));
    model.moveBefore(task("sub"), task("a"));
    expect(model.tasks.map((t) => [t.title, t.parentTitle])).toEqual([
      ["sub", "a"],
      ["a", undefined],
      ["b", undefined],
      ["c", undefined],
    ]);
  });

  it("keeps the order after reopening", async () => {
    model.moveBefore(task("c"), task("a"));
    await model.saved();
    expect(
      (await TaskFiles.open(storage)).store
        .filterGroups(model.filters[0].id)
        .flatMap((g) => g.takeTasks())
        .map((t) => t.title),
    ).toEqual(["c", "a", "b"]);
  });
});

describe("Blocked by", () => {
  beforeEach(() => {
    model.add("a");
    model.add("b");
  });
  const task = (title: string) => model.tasks.find((t) => t.title === title)!;

  it("marks a Task Blocked by another, listing the blocker", () => {
    expect(model.setBlockedBy(task("a"), [task("b").id])).toBe(true);
    expect(task("a").blocked).toBe(true);
    expect(task("a").blockedBy).toEqual([{ id: task("b").id, title: "b", closed: false }]);
  });

  it("removes a Blocked by", () => {
    model.setBlockedBy(task("a"), [task("b").id]);
    model.setBlockedBy(task("a"), []);
    expect(task("a")).toMatchObject({ blocked: false, blockedBy: [] });
  });

  it("refuses itself and a loop", () => {
    expect(model.setBlockedBy(task("a"), [task("a").id])).toBe(false);
    model.setBlockedBy(task("a"), [task("b").id]);
    expect(model.setBlockedBy(task("b"), [task("a").id])).toBe(false);
  });

  it("offers the Open Tasks it isn't Blocked by yet", () => {
    model.add("c");
    model.setBlockedBy(task("a"), [task("b").id]);
    expect(model.blockerCandidates(task("a")).map((t) => t.title)).toEqual(["c"]);
  });

  it("doesn't offer a Task that would make Tasks wait on each other", () => {
    model.setBlockedBy(task("b"), [task("a").id]);
    expect(model.blockerCandidates(task("a")).map((t) => t.title)).toEqual([]);
  });

  it("unblocks a Task once its blocker is Done", () => {
    model.setBlockedBy(task("a"), [task("b").id]);
    model.close(task("b"), Closing.Done);
    expect(task("a").blocked).toBe(false);
  });
});

describe("opening a Task by its id", () => {
  it("shows the list shown, with the Task to edit", () => {
    model.add("Later");
    const { id } = first();
    model.openLinked(id);
    expect(model.list).toEqual({ kind: "filter", id: model.filters[0].id });
    expect(model.editing?.id).toBe(id);
    model.openLinked(undefined);
    expect(model.editing).toBeUndefined();
  });

  it("shows the first Favorite over a Filter that doesn't hold the Task", () => {
    model.add("Later");
    const { id } = first();
    model.list = { kind: "filters" };
    model.openLinked(id);
    expect(model.list).toEqual({ kind: "filter", id: model.filters[0].id });
    expect(model.editing?.id).toBe(id);
  });

  it("shows the first Favorite for an unknown id", () => {
    model.add("Later");
    model.list = { kind: "filters" };
    model.openLinked("unknown");
    expect(model.list).toEqual({ kind: "filter", id: model.filters[0].id });
    expect(model.editing).toBeUndefined();
  });

  it("opens no Closed Task", () => {
    model.add("Parent");
    model.addSubtask("Sub", first());
    const sub = model.tasks[1];
    model.close(sub, Closing.Done);
    model.commit();
    model.openLinked(sub.id);
    expect(model.editing).toBeUndefined();
  });

  it("tells its listener", () => {
    model.add("Now");
    let changes = 0;
    model.onChange = () => changes++;
    model.openLinked(first().id);
    expect(changes).toBe(1);
  });
});

describe("opening Blockers and Subtasks", () => {
  /** "Top" (opened) Blocked by "Mid", itself Blocked by "Low". */
  function chain() {
    model.add("Low");
    model.add("Mid");
    model.add("Top");
    const low = model.tasks.find((t) => t.title === "Low")!;
    const mid = model.tasks.find((t) => t.title === "Mid")!;
    const top = model.tasks.find((t) => t.title === "Top")!;
    model.setBlockedBy(top, [mid.id]);
    model.setBlockedBy(mid, [low.id]);
    return { low, mid, top };
  }

  it("pushes a Blocker on the stack, to any depth, and pops it with back", () => {
    const { low, mid, top } = chain();
    model.openLinked(top.id);
    expect(model.open(mid.id)).toBe(true);
    expect(model.open(low.id)).toBe(true);
    expect(model.depth).toBe(3);
    expect(model.editing?.id).toBe(low.id);
    model.back();
    expect(model.editing?.id).toBe(mid.id);
    model.back();
    model.back();
    expect(model.editing?.id).toBe(top.id);
  });

  it("refuses a Closed Blocker", () => {
    const { mid, top } = chain();
    model.openLinked(top.id);
    model.close(mid, Closing.Done);
    model.commit();
    expect(model.open(mid.id)).toBe(false);
    expect(model.depth).toBe(1);
  });

  it("refuses opening while the Title on top is empty, and popping", () => {
    const { low, mid, top } = chain();
    model.openLinked(top.id);
    model.open(mid.id);
    model.edit(model.editing!, {
      title: " ",
      description: "",
      priority: 4,
      tags: [],
    });
    expect(model.isLeavingBlocked).toBe(true);
    expect(model.open(low.id)).toBe(false);
    model.back();
    expect(model.editing?.id).toBe(mid.id);
    model.edit(model.editing!, {
      title: "Mid",
      description: "",
      priority: 4,
      tags: [],
    });
    expect(model.isLeavingBlocked).toBe(false);
    expect(model.open(low.id)).toBe(true);
  });

  it("shows the latest saved state after back", () => {
    const { mid, top } = chain();
    model.openLinked(top.id);
    model.open(mid.id);
    model.edit(model.editing!, {
      title: "Renamed",
      description: "",
      priority: 1,
      tags: [],
    });
    model.back();
    expect(model.editing?.blockedBy.map((b) => b.title)).toEqual(["Renamed"]);
  });

  it("pops to the Task below, or starts over for another one, when opened by id", () => {
    const { low, mid, top } = chain();
    model.openLinked(top.id);
    model.open(mid.id);
    model.openLinked(top.id);
    expect(model.depth).toBe(1);
    model.open(mid.id);
    model.openLinked(mid.id);
    expect(model.depth).toBe(2);
    model.openLinked(low.id);
    expect(model.depth).toBe(1);
    model.openLinked(undefined);
    expect(model.depth).toBe(0);
  });
});

describe("Subtasks of a Task", () => {
  it("lists the direct Subtasks in order, without Rejected ones", () => {
    model.add("Parent");
    const parent = first();
    model.addSubtask("One", parent);
    model.addSubtask("Two", parent);
    model.addSubtask("Gone", parent);
    model.close(
      model.tasks.find((t) => t.title === "Gone")!,
      Closing.Rejected,
    );
    model.commit();
    expect(model.subtasks(parent.id).map((t) => t.title)).toEqual(["One", "Two"]);
  });

  it("keeps the order of Subtasks added in a row", () => {
    model.add("Parent");
    const parent = first();
    for (const title of ["A", "B", "C", "D"]) model.addSubtask(title, parent);
    expect(model.subtasks(parent.id).map((t) => t.title)).toEqual(["A", "B", "C", "D"]);
  });

  it("is empty for a Task without Subtasks or an unknown one", () => {
    model.add("Alone");
    expect(model.subtasks(first().id)).toEqual([]);
    expect(model.subtasks("unknown")).toEqual([]);
  });

  describe("closed in the section", () => {
    let parent: ListedTask;
    const open = () => model.find(parent.id)!.openSubtasks;
    beforeEach(() => {
      model.add("Parent");
      parent = first();
      model.addSubtask("One", parent);
      model.addSubtask("Two", parent);
    });

    it("keeps a Done Subtask, marked, but no longer counts it; Revert takes it back", () => {
      model.close(model.subtasks(parent.id)[0], Closing.Done);
      expect(model.subtasks(parent.id).map((t) => [t.title, t.done])).toEqual([
        ["One", true],
        ["Two", false],
      ]);
      expect(open()).toBe(1);
      model.revert();
      expect(model.subtasks(parent.id).map((t) => t.done)).toEqual([false, false]);
      expect(open()).toBe(2);
    });

    it("drops a Rejected Subtask from the section and the counter", () => {
      model.close(model.subtasks(parent.id)[0], Closing.Rejected);
      expect(model.subtasks(parent.id).map((t) => t.title)).toEqual(["Two"]);
      expect(open()).toBe(1);
      model.revert();
      expect(model.subtasks(parent.id)).toHaveLength(2);
    });
  });

  describe("reordered", () => {
    let parent: ListedTask;
    const sub = (title: string) => model.subtasks(parent.id).find((t) => t.title === title)!;
    const sectionTitles = () => model.subtasks(parent.id).map((t) => t.title);
    beforeEach(() => {
      model.add("Parent");
      parent = first();
      for (const title of ["A", "B", "C"]) model.addSubtask(title, parent);
    });

    it("moves a Subtask before a sibling in the section", () => {
      expect(model.moveSubtask(sub("C"), sub("A"))).toBe(true);
      expect(sectionTitles()).toEqual(["C", "A", "B"]);
    });

    it("moves a Subtask to the end with no sibling given", () => {
      model.moveSubtask(sub("A"), undefined);
      expect(sectionTitles()).toEqual(["B", "C", "A"]);
    });

    it("refuses a move before a Task that is not a sibling", () => {
      expect(model.moveSubtask(sub("A"), parent)).toBe(false);
      expect(sectionTitles()).toEqual(["A", "B", "C"]);
    });
  });

  it("opens a Subtask over its Parent, and back returns", () => {
    model.add("Parent");
    const parent = first();
    model.addSubtask("Sub", parent);
    model.openLinked(parent.id);
    const [sub] = model.subtasks(parent.id);
    expect(model.open(sub.id)).toBe(true);
    expect(model.editing?.id).toBe(sub.id);
    model.back();
    expect(model.editing?.id).toBe(parent.id);
  });

  it("opens a Subtask's Parent on top, or goes back to it when it is just below", () => {
    model.add("Parent");
    const parent = first();
    model.addSubtask("Sub", parent);
    const [sub] = model.subtasks(parent.id);
    model.openLinked(sub.id);
    expect(model.open(parent.id)).toBe(true);
    expect(model.depth).toBe(2);
    expect(model.editing?.id).toBe(parent.id);
    expect(model.open(sub.id)).toBe(true);
    expect(model.depth).toBe(1);
    expect(model.editing?.id).toBe(sub.id);
  });
});

describe("Sync", () => {
  it("shows the Tasks Sync wrote, and none it deleted", async () => {
    model.add("Mine");
    await model.saved();
    const other = await TaskFiles.open(new MemoryStorage());
    other.store.create("Theirs", 4, []);
    const [path] = other.store
      .takeChanges()
      .filter((p) => p.startsWith("Tasks/") && p.endsWith(".md"));
    await storage.write(path, new TextEncoder().encode(other.store.file(path)));
    await model.syncDidChange();
    expect(titles().sort()).toEqual(["Mine", "Theirs"]);

    await storage.remove(path);
    await model.syncDidChange();
    expect(titles()).toEqual(["Mine"]);
  });

  it("keeps a pending Close", async () => {
    model.add("Now");
    model.close(first(), Closing.Done);
    await model.syncDidChange();
    expect(model.tasks).toEqual([]);
    expect(model.pendingClose).toBeUndefined();
    await model.saved();
    expect(titles()).toEqual([]);
  });

  it("commits a Close made while it read the files", async () => {
    model.add("Now");
    await model.saved();
    const syncing = model.syncDidChange();
    model.close(first(), Closing.Rejected);
    await syncing;
    expect(model.pendingClose).toBeUndefined();
    expect(model.tasks).toEqual([]);
  });

  it("tells when a change was saved", async () => {
    let saved = 0;
    model.onSaved = () => saved++;
    model.add("Now");
    await model.saved();
    expect(saved).toBe(1);
  });
});

describe("Tags", () => {
  const tag = (name: string) => model.allTags.find((t) => t.name === name);

  beforeEach(() => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["work", "home"],
    });
  });

  it("lists every Tag alphabetically, in Default", () => {
    expect(model.allTags).toEqual([
      { name: "home", color: undefined },
      { name: "work", color: undefined },
    ]);
  });

  it("lists a Tag no Task carries any more", () => {
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: [],
    });
    expect(model.allTags.map((t) => t.name)).toEqual(["home", "work"]);
    expect(model.suggestTags("ho", []).matches).toEqual(["home"]);
  });

  it("sets a Tag Color, which the list's Tasks show at once", () => {
    const changed = vi.fn();
    model.onChange = changed;
    model.setTagColor("work", "#d0021b");
    expect(tag("work")?.color).toBe("#d0021b");
    expect(first().tags).toEqual(["work", "home"]);
    expect(first().tagColors).toEqual({ work: "#d0021b" });
    expect(changed).toHaveBeenCalled();
  });

  it("resets a Tag to Default", () => {
    model.setTagColor("work", "#d0021b");
    model.setTagColor("work", undefined);
    expect(tag("work")?.color).toBeUndefined();
    expect(first().tagColors).toEqual({});
  });

  it("deletes a Tag from every Task and forgets its Tag Color", async () => {
    model.addSubtask("Old", first());
    const old = model.tasks.find((t) => t.title === "Old")!;
    model.edit(old, {
      title: "Old",
      description: "",
      priority: 4,
      tags: ["work"],
    });
    const oldFile = async () => {
      await model.saved();
      return new TextDecoder().decode(await storage.read(`Tasks/${old.id}.md`));
    };
    expect(await oldFile()).toContain("  - work\n");
    model.setTagColor("work", "#d0021b");
    const changed = vi.fn();
    model.onChange = changed;
    model.deleteTag("work");
    expect(model.allTags.map((t) => t.name)).toEqual(["home"]);
    expect(first().tags).toEqual(["home"]);
    expect(await oldFile()).not.toContain("work");
    expect(changed).toHaveBeenCalled();
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["work"],
    });
    expect(tag("work")?.color).toBeUndefined();
  });

  it("keeps a Tag Color after a reload from the saved files", async () => {
    model.setTagColor("work", "#d0021b");
    await model.saved();
    const reopened = new TasksModel(await TaskFiles.open(storage));
    expect(reopened.allTags.find((t) => t.name === "work")?.color).toBe("#d0021b");
  });
});

describe("Filters", () => {
  const filterTitles = () => model.filters.map((f) => f.title);
  const filter = (title: string) => model.filters.find((f) => f.title === title)!;

  // Tasks created in one second tie on `created`: the clock moves a second per read.
  beforeEach(() => {
    let now = Date.now();
    vi.spyOn(Date, "now").mockImplementation(() => (now += 1000));
    deleteFavorites();
  });
  afterEach(() => vi.restoreAllMocks());

  it("starts with none, and shows the Filters list once chosen", () => {
    expect(model.filters).toEqual([]);
    model.list = { kind: "filters" };
    expect(model.list).toEqual({ kind: "filters" });
    expect(model.tasks).toEqual([]);
  });

  it("creates Filters at the end, refusing an empty Title and a Query that can't be read", () => {
    expect(model.addFilter("Work", "@work")).toBe(true);
    expect(model.addFilter("Home", "@home | @garden")).toBe(true);
    expect(model.addFilter("  ", "@home")).toBe(false);
    expect(model.addFilter("Bad", "@work &")).toBe(false);
    expect(filterTitles()).toEqual(["Work", "Home"]);
    expect(filter("Work")).toMatchObject({
      query: "@work",
      favorite: false,
      error: undefined,
    });
  });

  it("allows two Filters of one Title", () => {
    expect(model.addFilter("Work", "@a")).toBe(true);
    expect(model.addFilter("Work", "@b")).toBe(true);
    expect(model.filters).toHaveLength(2);
  });

  it("counts the Open Tasks each Filter matches, the Backlog included", () => {
    model.add("A", { tags: ["work"] });
    model.add("B", { tags: ["work", "home"] });
    model.add("C", { tags: ["work"] });
    model.addFilter("Work", "@work");
    model.addFilter("Only work", "@work & !@home");
    model.addFilter("None", "@nothing");
    expect(model.filters.map((f) => f.matches)).toEqual([3, 2, 0]);
    model.add("D", { tags: ["work"] });
    expect(filter("Work").matches).toBe(4);
  });

  it("gives a Task created in a Filter the Tags its Query requires", () => {
    model.addFilter("Work", "@work & !@review");
    model.addFilter("Either", "@a | @b");
    model.list = { kind: "filter", id: filter("Work").id };
    expect(model.add("A", { tags: ["x", "work"] })).toBe(true);
    expect([...model.tasks[0].tags].sort()).toEqual(["work", "x"]);
    model.list = { kind: "filter", id: filter("Either").id };
    model.add("B");
    model.addFilter("All", "*");
    model.list = { kind: "filter", id: filter("All").id };
    model.add("C");
    expect(model.tasks.map((t) => [t.title, t.tags.length])).toEqual([
      ["A", 2],
      ["B", 0],
      ["C", 0],
    ]);
  });

  it("edits the Title and Query, with the refusals of create", () => {
    model.addFilter("Work", "@work");
    const { id } = filter("Work");
    expect(model.editFilter(id, "Job", "@job", "")).toBe(true);
    expect(filter("Job")).toMatchObject({ id, query: "@job" });
    expect(model.editFilter(id, "", "@job", "")).toBe(false);
    expect(model.editFilter(id, "Job", "(@job", "")).toBe(false);
    expect(filter("Job").query).toBe("@job");
  });

  it("moves a Filter before another, or to the end", () => {
    for (const title of ["A", "B", "C"]) model.addFilter(title, "@x");
    model.moveFilter(filter("C"), filter("A"));
    expect(filterTitles()).toEqual(["C", "A", "B"]);
    model.moveFilter(filter("C"), undefined);
    expect(filterTitles()).toEqual(["A", "B", "C"]);
  });

  it("shows a Filter's Open Tasks, the Backlog's and Subtasks on their own, in the order of creation", () => {
    model.add("A", { tags: ["work"] });
    model.add("B");
    model.add("C", { tags: ["work"] });
    model.add("D", { tags: ["work"] });
    model.addFilter("Work", "@work");
    model.list = { kind: "filter", id: filter("Work").id };
    expect(titles()).toEqual(["A", "C", "D"]);
    model.addSubtask("Sub", model.tasks[0], { tags: ["work"] });
    expect(model.tasks.find((t) => t.title === "Sub")?.parentTitle).toBe("A");
  });

  it("closes and adds Tasks inside a Filter, which stays shown", () => {
    model.add("A", { tags: ["work"] });
    model.addFilter("Work", "@work");
    const list = { kind: "filter" as const, id: filter("Work").id };
    model.list = list;
    model.add("Plain", { tags: ["work"] });
    expect(model.list).toEqual(list);
    expect(titles()).toEqual(["A", "Plain"]);
    model.close(first(), Closing.Done);
    expect(titles()).toEqual(["A", "Plain"]); // a pending Done stays, marked
    expect(filter("Work").matches).toBe(2);
  });

  it("orders Tasks within a Filter only", () => {
    for (const title of ["A", "B", "C"]) model.add(title, { tags: ["work"] });
    model.addFilter("Work", "@work");
    model.list = { kind: "filter", id: filter("Work").id };
    model.moveBefore(model.tasks[2], model.tasks[0]);
    expect(titles()).toEqual(["C", "A", "B"]);
    model.addFilter("All", "*");
    model.list = { kind: "filter", id: filter("All").id };
    expect(titles()).toEqual(["A", "B", "C"]);
  });

  it("changes a Subtask's order in the sheet when it is dragged past a sibling in a Filter", () => {
    model.addFilter("Work", "@work");
    model.list = { kind: "filter", id: filter("Work").id };
    model.add("P", { tags: ["work"] });
    for (const title of ["x", "y"]) model.addSubtask(title, first());
    expect(titles()).toEqual(["P", "x", "y"]);
    model.moveBefore(model.tasks[2], model.tasks[1]);
    expect(titles()).toEqual(["P", "y", "x"]);
    expect(model.subtasks(first().id).map((t) => t.title)).toEqual(["y", "x"]);
  });

  describe("Sort", () => {
    beforeEach(() => {
      for (const [title, priority] of [
        ["low", 4],
        ["high", 1],
        ["mid", 2],
      ] as const)
        model.add(title, { priority, tags: ["work"] });
      model.addFilter("Work", "@work");
      model.list = { kind: "filter", id: filter("Work").id };
    });

    it("is Manual at first, and sorts by Priority in either Direction", () => {
      expect(filter("Work").sort).toEqual({
        key: FilterSortKey.Manual,
        direction: undefined,
      });
      expect(titles()).toEqual(["low", "high", "mid"]);
      model.setSort(filter("Work"), FilterSortKey.Priority);
      expect(filter("Work").sort).toEqual({
        key: FilterSortKey.Priority,
        direction: Direction.Ascending,
      });
      expect(titles()).toEqual(["high", "mid", "low"]);
      model.setSort(filter("Work"), FilterSortKey.Priority, Direction.Descending);
      expect(titles()).toEqual(["low", "mid", "high"]);
    });

    it("keeps the Direction when Priority is picked again, and the order shown for Manual", () => {
      model.setSort(filter("Work"), FilterSortKey.Priority, Direction.Descending);
      model.setSort(filter("Work"), FilterSortKey.Manual);
      expect(filter("Work").sort.direction).toBeUndefined();
      expect(titles()).toEqual(["low", "mid", "high"]);
    });

    it("turns Manual when a drag crosses Priority groups", () => {
      model.setSort(filter("Work"), FilterSortKey.Priority);
      model.moveBefore(model.tasks[2], model.tasks[0]);
      expect(filter("Work").sort.key).toBe(FilterSortKey.Manual);
      expect(titles()).toEqual(["low", "high", "mid"]);
    });

    it("lists the keys the core has, Manual first", () => {
      expect(sortKeys()).toEqual([FilterSortKey.Manual, FilterSortKey.Priority]);
    });
  });

  it("keeps the Filter shown when a Task of it is opened, and leaves one that is Deleted", () => {
    model.add("A", { tags: ["work"] });
    model.addFilter("Work", "@work");
    const list = { kind: "filter" as const, id: filter("Work").id };
    model.list = list;
    model.openLinked(first().id);
    expect(model.list).toEqual(list);
    expect(model.editing?.title).toBe("A");
    model.openLinked(undefined);
    model.deleteFilter(filter("Work"));
    expect(model.list).toEqual({ kind: "filters" });
    expect(model.filters).toEqual([]);
    expect(titles()).toEqual([]);
  });

  it("offers the Favorites in the Filters' order, then Filters", () => {
    expect(model.bar.map((e) => e.title)).toEqual(["Filters"]);
    for (const title of ["A", "B", "C"]) model.addFilter(title, "@a");
    model.setFavorite(filter("C"), true);
    model.setFavorite(filter("A"), true);
    expect(model.filter).toBeUndefined();
    expect(filter("A").favorite).toBe(true);
    expect(model.bar.map((e) => e.title)).toEqual(["A", "C", "Filters"]);
    model.moveFilter(filter("C"), filter("A"));
    expect(model.bar.map((e) => e.title)).toEqual(["C", "A", "Filters"]);
    model.setFavorite(filter("C"), false);
    expect(model.bar.map((e) => e.title)).toEqual(["A", "Filters"]);
  });

  it("puts a Filter that is no Favorite under Filters", () => {
    model.addFilter("A", "@a");
    model.list = { kind: "filter", id: filter("A").id };
    expect(model.barEntry).toEqual({ kind: "filters" });
    model.setFavorite(filter("A"), true);
    expect(model.barEntry).toEqual({ kind: "filter", id: filter("A").id });
    model.list = { kind: "filters" };
    expect(model.barEntry).toEqual({ kind: "filters" });
  });

  it("saves a Favorite and reads it again", async () => {
    model.addFilter("A", "@a");
    model.setFavorite(filter("A"), true);
    await model.saved();
    const reopened = new TasksModel(await TaskFiles.open(storage));
    expect(reopened.filters[0].favorite).toBe(true);
  });

  it("opens the Device's last list, and the first Favorite if its Filter is gone", async () => {
    const values = new Map<string, string>();
    const device = {
      getItem: (key: string) => values.get(key) ?? null,
      setItem: (key: string, value: string) => void values.set(key, value),
    };
    const open = async () => new TasksModel(await TaskFiles.open(storage), 5000, device);
    const first = await open();
    expect(first.list).toEqual({ kind: "filters" }); // no Favorite
    first.addFilter("Fav", "*");
    first.setFavorite(first.filters[0], true);
    first.addFilter("A", "@a");
    await first.saved();
    const id = first.filters.find((f) => f.title === "A")!.id;
    for (const list of [{ kind: "filters" as const }, { kind: "filter" as const, id }]) {
      first.list = list;
      expect((await open()).list).toEqual(list);
    }
    first.deleteFilter(first.filters.find((f) => f.title === "A")!);
    first.list = { kind: "filter", id };
    expect((await open()).list).toEqual({
      kind: "filter",
      id: first.filters[0].id,
    });
    // The Filters list when there is no Favorite.
    first.deleteFilter(first.filters[0]);
    expect((await open()).list).toEqual({ kind: "filters" });
  });

  it("Deletes a Filter after which every Task is as it was", () => {
    model.add("A", { tags: ["work"] });
    model.addFilter("Work", "@work");
    model.deleteFilter(filter("Work"));
    model.addFilter("All", "*");
    model.list = { kind: "filter", id: filter("All").id };
    expect(titles()).toEqual(["A"]);
    expect(first().tags).toEqual(["work"]);
  });

  it("saves Filters and their removal, and reads them again", async () => {
    model.addFilter("Work", "@work");
    model.addFilter("Home", "@home");
    await model.saved();
    expect((await storage.files("Filters")).map((f) => f.name)).toHaveLength(2);
    model.deleteFilter(filter("Home"));
    await model.saved();
    expect((await storage.files("Filters")).map((f) => f.name)).toHaveLength(1);
    const reopened = new TasksModel(await TaskFiles.open(storage));
    expect(reopened.filters.map((f) => f.title)).toEqual(["Work"]);
  });

  it("lists a Filter whose stored Query can't be read with its error, matching nothing", async () => {
    model.add("A", { tags: ["work"] });
    model.addFilter("Work", "@work");
    await model.saved();
    const [{ name }] = await storage.files("Filters");
    const path = `Filters/${name}`;
    const text = new TextDecoder().decode(await storage.read(path)).replace("@work", "@work &");
    await storage.write(path, new TextEncoder().encode(text));
    await model.syncDidChange();
    expect(filter("Work").error).toBeDefined();
    expect(filter("Work").matches).toBe(0);
    expect(model.editFilter(filter("Work").id, "Work", "@work", "")).toBe(true);
    expect(filter("Work").error).toBeUndefined();
  });

  it("follows Filters that Sync wrote", async () => {
    model.addFilter("Work", "@work");
    await model.saved();
    const [{ name }] = await storage.files("Filters");
    await storage.remove(`Filters/${name}`);
    await model.syncDidChange();
    expect(model.filters).toEqual([]);
  });
});

describe("a Query being typed", () => {
  beforeEach(() => {
    deleteFavorites();
    model.add("A", { tags: ["work", "home"] });
  });

  it("reports why it can't be read, with the position, and what Tags it names are unknown", () => {
    expect(model.checkQuery("@a & (").problem).toEqual({
      position: 6,
      message: "Expected @tag, ! or (",
    });
    expect(model.checkQuery("@work | !@nope")).toEqual({
      problem: undefined,
      unknown: ["nope"],
    });
  });

  it("makes typed text lowercase and refuses what neither a Tag nor the Query holds", () => {
    expect(TasksModel.allowedQueryText("@Work & !(@Home)")).toBe("@work & !(@home)");
    expect(TasksModel.allowedQueryText("@wo,rk")).toBe("@work");
  });

  it("suggests the known Tags for the Tag being typed, and completes it", () => {
    expect(model.suggestQueryTags("@work & @h")).toEqual(["home"]);
    expect(model.suggestQueryTags("@work & ")).toEqual([]);
    expect(TasksModel.completingQuery("@work & @h", "home")).toBe("@work & @home");
  });

  it("takes Wildcards, suggesting nothing for them and marking those that fit no Tag", () => {
    model.add("B", { tags: ["work-backlog"] });
    expect(TasksModel.allowedQueryText("@*Backlog")).toBe("@*backlog");
    expect(model.suggestQueryTags("@*back")).toEqual([]);
    expect(model.checkQuery("@*backlog | @*nope*")).toEqual({
      problem: undefined,
      unknown: ["*nope*"],
    });
    model.addFilter("Backlogs", "@*backlog");
    expect(model.filters[0].matches).toBe(1);
  });

  it("leaves a Query naming a Tag as it is when the Tag is Deleted", () => {
    model.addFilter("Work", "!@work");
    model.deleteTag("work");
    expect(model.filters[0].query).toBe("!@work");
  });
});

describe("a Grouped Filter", () => {
  const groupsShown = () => model.groups.map((g) => [g.tag, g.tasks.map((t) => t.title)]);

  beforeEach(() => {
    deleteFavorites();
    model.add("Loose");
    model.add("W", { tags: ["work"] });
    model.add("Both", { tags: ["work", "home"] });
    model.setTagColor("work", "#112233");
    model.addFilter("All", "*");
  });

  it("shows the Tasks without a Group first, then a Group per Tag with its Tag Color, empty ones too", () => {
    expect(model.addFilter("Grouped", "*", "@home @work")).toBe(true);
    model.list = {
      kind: "filter",
      id: model.filters.find((f) => f.title === "Grouped")!.id,
    };
    expect(model.groups.map((g) => [g.tag, g.color])).toEqual([
      [undefined, undefined],
      ["home", undefined],
      ["work", "#112233"],
    ]);
    expect(groupsShown()).toEqual([
      [undefined, ["Loose"]],
      ["home", ["Both"]],
      ["work", ["W", "Both"]],
    ]);
  });

  it("moves a Task in one Group only, leaving its place in other Groups", () => {
    model.add("H", { tags: ["home"] });
    model.addFilter("Grouped", "*", "@home @work");
    model.list = {
      kind: "filter",
      id: model.filters.find((f) => f.title === "Grouped")!.id,
    };
    const both = model.tasks.find((t) => t.title === "Both")!;
    const w = model.tasks.find((t) => t.title === "W")!;
    model.moveBefore(both, w, "work");
    expect(groupsShown()).toEqual([
      [undefined, ["Loose"]],
      ["home", ["Both", "H"]],
      ["work", ["Both", "W"]],
    ]);
  });

  it("is one Group without Group by", () => {
    model.list = { kind: "filter", id: model.filters[0].id };
    expect(groupsShown()).toEqual([[undefined, ["Loose", "W", "Both"]]]);
  });

  it("checks Group by without saving, and refuses an unknown Tag", () => {
    expect(model.checkGroupBy("@work @home")).toBeUndefined();
    expect(model.checkGroupBy("@nope")).toMatch(/nope/);
    expect(model.addFilter("Bad", "*", "@nope")).toBe(false);
    expect(model.filters.some((f) => f.title === "Bad")).toBe(false);
  });

  it("suggests Tags for Group by, leaving out those listed", () => {
    expect(model.suggestGroupByTags("@w")).toEqual(["work"]);
    expect(model.suggestGroupByTags("@work @")).toEqual(["home"]);
    expect(model.suggestGroupByTags("@work ")).toEqual([]);
  });

  it("edits Group by, clearing it with empty text", () => {
    const { id } = model.filters[0];
    expect(model.editFilter(id, "All", "*", "@work")).toBe(true);
    expect(model.filters.find((f) => f.id === id)!.groupBy).toBe("@work");
    expect(model.editFilter(id, "All", "*", "")).toBe(true);
    expect(model.filters.find((f) => f.id === id)!.groupBy).toBe("");
  });
});

describe("releasing Images", () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2]);
  const kept = async (reference: string) =>
    (await storage.read(`Tasks/${reference}`)) !== undefined;
  const settle = () => model.syncDidChange(); // waits for the saves

  it("deletes an Image removed from the Description, and keeps one still shown", async () => {
    const shown = model.addImage(png)!;
    const removed = model.addImage(png)!;
    model.add("a", { description: `![](${shown})\n\n![](${removed})` });
    model.openLinked(first().id);
    model.edit(first(), {
      title: "a",
      description: `![](${shown})`,
      priority: 4,
      tags: [],
    });
    model.releaseImages();
    await settle();
    expect(await kept(shown)).toBe(true);
    expect(await kept(removed)).toBe(false);
  });

  it("deletes an Image the Description started with once it is removed", async () => {
    const image = model.addImage(png)!;
    model.add("a", { description: `![](${image})` });
    model.releaseImages();
    model.openLinked(first().id);
    model.edit(first(), { title: "a", description: "", priority: 4, tags: [] });
    model.releaseImages();
    await settle();
    expect(await kept(image)).toBe(false);
  });

  it("deletes the Images pasted into a cancelled New Task", async () => {
    const image = model.addImage(png)!;
    model.releaseImages();
    await settle();
    expect(await kept(image)).toBe(false);
  });
});
