import { readFileSync } from "node:fs";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { type Core, Planning, loadCore } from "../core/Core";
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
    model.edit(first(), { title: "Buy milk", description: "", priority: 4, tags: ["a", "b"] });
    model.edit(first(), { title: "Buy milk", description: "", priority: 4, tags: ["b"] });
    expect(first().tags).toEqual(["b"]);
  });

  it("saves nothing if the Title is empty or a Tag is invalid", () => {
    model.add("Buy milk");
    const task = first();
    expect(model.edit(task, { title: " ", description: "x", priority: 1, tags: [] })).toBe(false);
    expect(model.edit(task, { title: "New", description: "x", priority: 1, tags: ["No!"] })).toBe(
      false,
    );
    expect(first()).toMatchObject({ title: "Buy milk", description: "", priority: 4 });
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
    model.edit(first(), { title: "Buy milk", description: "", priority: 2, tags: ["home"] });
    await model.saved();
    const reopened = new TasksModel(await TaskFiles.open(core, storage));
    expect(reopened.tasks[0]).toMatchObject({ title: "Buy milk", priority: 2, tags: ["home"] });
  });

  it("reports a failed save", async () => {
    storage.write = () => Promise.reject(new Error("full"));
    model.add("Buy milk");
    await model.saved();
    expect(model.failure).toBeDefined();
  });
});
