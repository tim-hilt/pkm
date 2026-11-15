import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, Closing, Planning, TaskQuickAdd, loadCore } from "../core/Core";
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
  model = new TasksModel(await TaskFiles.open(core, storage));
});

const titles = () => model.tasks.map((task) => task.title);
const first = () => model.tasks[0];

describe("the Tasks list", () => {
  it("starts on the Active list, empty", () => {
    expect(model.planning).toBe(Planning.Active);
    expect(model.tasks).toEqual([]);
  });

  it("adds a Task, Active by default, with Priority 4 and no Tags", () => {
    expect(model.add("Buy milk")).toBe(true);
    expect(first()).toMatchObject({
      title: "Buy milk",
      priority: 4,
      description: "",
      tags: [],
      planning: Planning.Active,
    });
  });

  it("adds a Task with a Description", () => {
    expect(model.add("Buy milk", Planning.Active, { description: "Two litres" })).toBe(true);
    expect(first()).toMatchObject({ title: "Buy milk", description: "Two litres" });
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
    expect(model.add(title, Planning.Active, { priority: quickAdd.priority(), tags })).toBe(true);
    quickAdd.free();
    expect(first()).toMatchObject({ title: "Fix login", priority: 1, tags: ["bug", "work/x"] });
    expect(model.allTags.map((t) => t.name)).toEqual(["bug", "work/x"]);
  });

  it("adds a Subtask with the Priority and Tags of Quick Add, and a Description", () => {
    model.add("Shop");
    const quickAdd = new TaskQuickAdd();
    quickAdd.replace(0, 0, "p2 Milk @food ");
    const extras = { description: "Oat", priority: quickAdd.priority(), tags: quickAdd.tags() };
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
    expect(model.add(quickAdd.title(), Planning.Active, { tags: quickAdd.tags() })).toBe(false);
    quickAdd.free();
    expect(model.tasks).toEqual([]);
  });

  it("refuses an empty Title and adds nothing", () => {
    expect(model.add("   ")).toBe(false);
    expect(model.tasks).toEqual([]);
  });

  it("shows the Backlog when a Task is added there", () => {
    model.add("Learn Rust", Planning.Backlog);
    expect(model.planning).toBe(Planning.Backlog);
    expect(titles()).toEqual(["Learn Rust"]);
    model.planning = Planning.Active;
    expect(titles()).toEqual([]);
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
    expect(model.edit(first(), { title: " ", description: "x", priority: 1, tags: ["a"] })).toBe(
      true,
    );
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
      model.edit(first(), { title: "New", description: "x", priority: 1, tags: ["No!"] }),
    ).toBe(false);
    expect(first()).toMatchObject({ title: "Buy milk", description: "", priority: 4 });
  });

  it("edits again from a stale Task, as an autosaving view does", () => {
    model.add("Buy milk");
    const stale = first();
    const edit = (tags: string[]) =>
      model.edit(stale, { title: "Buy milk", description: "", priority: 4, tags });
    edit(["a"]);
    edit([]);
    expect(first().tags).toEqual([]);
  });

  it("moves a Task between the lists; it leaves the one shown", () => {
    model.add("Buy milk");
    model.plan(first(), Planning.Backlog);
    expect(titles()).toEqual([]);
    model.planning = Planning.Backlog;
    expect(titles()).toEqual(["Buy milk"]);
    model.plan(first(), Planning.Active);
    expect(titles()).toEqual([]);
  });

  it("suggests the known Tags that start with what is typed, then those containing it", () => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["home", "work"],
    });
    expect(model.suggestTags("h", [])).toEqual({ matches: ["home"], create: true });
    expect(model.suggestTags("", [])).toEqual({ matches: ["home", "work"], create: false });
    expect(model.suggestTags("o", [])).toEqual({ matches: ["home", "work"], create: true });
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
    model.plan(first(), Planning.Backlog);
    model.planning = Planning.Backlog;
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
    const reopened = new TasksModel(await TaskFiles.open(core, storage));
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

  const reopened = async () => new TasksModel(await TaskFiles.open(core, storage));

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
    expect(
      model.tasks.map((t) => [t.title, t.parentTitle, t.subtasksDone, t.subtasksTotal]),
    ).toEqual([
      ["parent", undefined, 0, 1],
      ["child", "parent", 0, 0],
    ]);
  });

  it("shows both markers on a Task that is Parent and Subtask, and counts Done Subtasks", () => {
    model.addSubtask("grandchild", model.tasks[1]);
    model.close(model.tasks[2], Closing.Done);
    model.commit();
    expect(model.tasks.map((t) => t.title)).toEqual(["parent", "child"]);
    expect(model.tasks[1]).toMatchObject({
      parentTitle: "parent",
      subtasksDone: 1,
      subtasksTotal: 1,
    });
  });

  it("refuses an empty Subtask Title", () => {
    expect(model.addSubtask(" ", first())).toBe(false);
  });

  it("follows the Parent to the Backlog", () => {
    model.plan(first(), Planning.Backlog);
    model.planning = Planning.Backlog;
    expect(titles()).toEqual(["parent", "child"]);
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
      (await TaskFiles.open(core, storage)).store.tasks(Planning.Active).map((t) => t.title),
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

  it("offers the Open Tasks it isn't Blocked by yet, of both lists", () => {
    model.add("c", Planning.Backlog);
    model.planning = Planning.Active;
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
  it("shows the list the Task is in, with the Task to edit", () => {
    model.add("Later", Planning.Backlog);
    const { id } = first();
    model.planning = Planning.Active;
    model.openLinked(id);
    expect(model.planning).toBe(Planning.Backlog);
    expect(model.editing?.id).toBe(id);
    model.openLinked(undefined);
    expect(model.editing).toBeUndefined();
  });

  it("shows the Active list for an unknown id", () => {
    model.add("Later", Planning.Backlog);
    model.openLinked("unknown");
    expect(model.planning).toBe(Planning.Active);
    expect(model.editing).toBeUndefined();
  });

  it("opens no Closed Task", () => {
    model.add("Parent");
    model.addSubtask("Sub", first());
    const sub = model.tasks[1];
    model.close(sub, Closing.Done);
    model.commit();
    model.openLinked(sub.id);
    expect(model.planning).toBe(Planning.Active);
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
  /** "Top" (opened) Blocked by "Mid", itself Blocked by "Low", the last of which is in the Backlog. */
  function chain() {
    model.add("Low", Planning.Backlog);
    model.add("Mid");
    model.add("Top");
    model.planning = Planning.Backlog;
    const low = model.tasks.find((t) => t.title === "Low")!;
    model.planning = Planning.Active;
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

  it("leaves the list shown when the Blocker is on the other list", () => {
    const { low, mid, top } = chain();
    model.openLinked(mid.id);
    expect(model.open(low.id)).toBe(true);
    expect(model.planning).toBe(Planning.Active);
    expect(top.planning).toBe(Planning.Active);
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
    model.edit(model.editing!, { title: " ", description: "", priority: 4, tags: [] });
    expect(model.isLeavingBlocked).toBe(true);
    expect(model.open(low.id)).toBe(false);
    model.back();
    expect(model.editing?.id).toBe(mid.id);
    model.edit(model.editing!, { title: "Mid", description: "", priority: 4, tags: [] });
    expect(model.isLeavingBlocked).toBe(false);
    expect(model.open(low.id)).toBe(true);
  });

  it("shows the latest saved state after back", () => {
    const { mid, top } = chain();
    model.openLinked(top.id);
    model.open(mid.id);
    model.edit(model.editing!, { title: "Renamed", description: "", priority: 1, tags: [] });
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
    expect(model.planning).toBe(Planning.Backlog);
    model.openLinked(undefined);
    expect(model.depth).toBe(0);
  });
});

describe("Subtasks of a Task", () => {
  it("lists the direct Subtasks in order, from either list, without Rejected ones", () => {
    model.add("Parent", Planning.Backlog);
    const parent = first();
    model.addSubtask("One", parent);
    model.addSubtask("Two", parent);
    model.addSubtask("Gone", parent);
    model.close(
      model.tasks.find((t) => t.title === "Gone")!,
      Closing.Rejected,
    );
    model.commit();
    model.planning = Planning.Active;
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
    const counts = () => {
      const { subtasksDone, subtasksTotal } = model.find(parent.id)!;
      return [subtasksDone, subtasksTotal];
    };
    beforeEach(() => {
      model.add("Parent");
      parent = first();
      model.addSubtask("One", parent);
      model.addSubtask("Two", parent);
    });

    it("keeps a Done Subtask, marked, and counts it; Revert takes it back", () => {
      model.close(model.subtasks(parent.id)[0], Closing.Done);
      expect(model.subtasks(parent.id).map((t) => [t.title, t.done])).toEqual([
        ["One", true],
        ["Two", false],
      ]);
      expect(counts()).toEqual([1, 2]);
      model.revert();
      expect(model.subtasks(parent.id).map((t) => t.done)).toEqual([false, false]);
      expect(counts()).toEqual([0, 2]);
    });

    it("drops a Rejected Subtask from the section and the counter", () => {
      model.close(model.subtasks(parent.id)[0], Closing.Rejected);
      expect(model.subtasks(parent.id).map((t) => t.title)).toEqual(["Two"]);
      expect(counts()).toEqual([0, 1]);
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

    it("moves a Subtask before a sibling, in the section and the list", () => {
      expect(model.moveSubtask(sub("C"), sub("A"))).toBe(true);
      expect(sectionTitles()).toEqual(["C", "A", "B"]);
      expect(model.tasks.map((t) => t.title)).toEqual(["Parent", "C", "A", "B"]);
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
});

describe("Sync", () => {
  it("shows the Tasks Sync wrote, and none it deleted", async () => {
    model.add("Mine");
    await model.saved();
    const other = await TaskFiles.open(core, new MemoryStorage());
    other.store.create("Theirs", Planning.Active, 4, []);
    const [path] = other.store.takeChanges().filter((p) => p.endsWith(".md"));
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
    model.edit(first(), { title: "Buy milk", description: "", priority: 4, tags: [] });
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

  it("deletes a Tag from every Task, Open or Closed, and forgets its Tag Color", async () => {
    model.addSubtask("Old", first());
    const old = model.tasks.find((t) => t.title === "Old")!;
    model.edit(old, { title: "Old", description: "", priority: 4, tags: ["work"] });
    model.close(old, Closing.Done);
    model.commit();
    const closedFile = async () => {
      await model.saved();
      return new TextDecoder().decode(await storage.read(`Tasks/${old.id}.md`));
    };
    expect(await closedFile()).toContain("state: done");
    expect(await closedFile()).toContain("  - work\n");
    model.setTagColor("work", "#d0021b");
    const changed = vi.fn();
    model.onChange = changed;
    model.deleteTag("work");
    expect(model.allTags.map((t) => t.name)).toEqual(["home"]);
    expect(first().tags).toEqual(["home"]);
    expect(await closedFile()).not.toContain("work");
    expect(changed).toHaveBeenCalled();
    model.edit(first(), { title: "Buy milk", description: "", priority: 4, tags: ["work"] });
    expect(tag("work")?.color).toBeUndefined();
  });

  it("keeps a Tag Color after a reload from the saved files", async () => {
    model.setTagColor("work", "#d0021b");
    await model.saved();
    const reopened = new TasksModel(await TaskFiles.open(core, storage));
    expect(reopened.allTags.find((t) => t.name === "work")?.color).toBe("#d0021b");
  });
});
