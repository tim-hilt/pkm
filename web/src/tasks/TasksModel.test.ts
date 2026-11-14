import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, Closing, Planning, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "./TaskFiles";
import { TasksModel } from "./TasksModel";

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

  it("saves nothing if the Title is empty or a Tag is invalid", () => {
    model.add("Buy milk");
    const task = first();
    expect(model.edit(task, { title: " ", description: "x", priority: 1, tags: [] })).toBe(false);
    expect(
      model.edit(task, {
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

  it("moves a Task between the lists; it leaves the one shown", () => {
    model.add("Buy milk");
    model.plan(first(), Planning.Backlog);
    expect(titles()).toEqual([]);
    model.planning = Planning.Backlog;
    expect(titles()).toEqual(["Buy milk"]);
    model.plan(first(), Planning.Active);
    expect(titles()).toEqual([]);
  });

  it("suggests the known Tags that start with what is typed", () => {
    model.add("Buy milk");
    model.edit(first(), {
      title: "Buy milk",
      description: "",
      priority: 4,
      tags: ["home", "work"],
    });
    expect(model.knownTags("h")).toEqual(["home"]);
    expect(model.knownTags("")).toEqual(["home", "work"]);
    expect(model.knownTags("x")).toEqual([]);
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

  it("lists a Subtask indented under its Parent", () => {
    expect(model.tasks.map((t) => [t.title, t.depth, t.hasSubtasks])).toEqual([
      ["parent", 0, true],
      ["child", 1, false],
    ]);
  });

  it("refuses an empty Subtask Title", () => {
    expect(model.addSubtask(" ", first())).toBe(false);
  });

  it("hides the Subtasks of a collapsed Parent", () => {
    model.toggleCollapsed(first());
    expect(model.isCollapsed(first())).toBe(true);
    expect(model.rows.map((t) => t.title)).toEqual(["parent"]);
    model.toggleCollapsed(first());
    expect(model.rows.map((t) => t.title)).toEqual(["parent", "child"]);
  });

  it("expands a Parent that got a new Subtask", () => {
    model.toggleCollapsed(first());
    model.addSubtask("second", first());
    expect(model.rows.map((t) => t.title)).toEqual(["parent", "child", "second"]);
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

  it("shows a Closed Subtask under its Open Parent, marked", () => {
    model.close(model.tasks[1], Closing.Done);
    model.commit();
    expect(model.tasks.map((t) => [t.title, t.closed])).toEqual([
      ["parent", false],
      ["child", true],
    ]);
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

  it("moves a Task under another, and back to the top level", () => {
    model.moveUnder(task("c"), task("a"));
    expect(model.tasks.map((t) => [t.title, t.depth])).toEqual([
      ["a", 0],
      ["c", 1],
      ["b", 0],
    ]);
    model.moveBefore(task("c"), undefined);
    expect(task("c").depth).toBe(0);
  });

  it("refuses moving a Task under its own Subtask, changing nothing", () => {
    model.moveUnder(task("b"), task("a"));
    model.moveUnder(task("a"), task("b"));
    expect(titles()).toEqual(["a", "b", "c"]);
    expect(task("a").depth).toBe(0);
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
    model.open(id);
    expect(model.planning).toBe(Planning.Backlog);
    expect(model.editing?.id).toBe(id);
    model.open(undefined);
    expect(model.editing).toBeUndefined();
  });

  it("shows the Active list for an unknown id", () => {
    model.add("Later", Planning.Backlog);
    model.open("unknown");
    expect(model.planning).toBe(Planning.Active);
    expect(model.editing).toBeUndefined();
  });

  it("opens no Closed Task", () => {
    model.add("Parent");
    model.addSubtask("Sub", first());
    const sub = model.tasks[1];
    model.close(sub, Closing.Done);
    model.commit();
    model.open(sub.id);
    expect(model.planning).toBe(Planning.Active);
    expect(model.editing).toBeUndefined();
  });

  it("tells its listener", () => {
    model.add("Now");
    let changes = 0;
    model.onChange = () => changes++;
    model.open(first().id);
    expect(changes).toBe(1);
  });
});

describe("Sync", () => {
  it("shows the Tasks Sync wrote, and none it deleted", async () => {
    model.add("Mine");
    await model.saved();
    const other = await TaskFiles.open(core, new MemoryStorage());
    other.store.create("Theirs", Planning.Active);
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
