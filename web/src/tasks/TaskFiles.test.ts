import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, Planning, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "./TaskFiles";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const titles = (files: TaskFiles, planning: Planning) => {
  const rows = files.store.tasks(planning);
  const found = rows.map((row) => row.title);
  rows.forEach((row) => row.free());
  return found;
};

describe("the Tasks in storage", () => {
  it("keeps a created Task in the Tasks folder, and finds it again", async () => {
    const storage = new MemoryStorage();
    const first = await TaskFiles.open(core, storage);
    first.store.create("Buy milk", Planning.Active);
    await first.save();
    expect((await storage.files("Tasks")).map((file) => file.name)).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    const second = await TaskFiles.open(core, storage);
    expect(titles(second, Planning.Active)).toEqual(["Buy milk"]);
  });

  it("appends to the History of this Device across launches", async () => {
    const storage = new MemoryStorage();
    const first = await TaskFiles.open(core, storage);
    first.store.create("One", Planning.Active);
    await first.save();
    const second = await TaskFiles.open(core, storage);
    second.store.create("Two", Planning.Active);
    await second.save();

    const logs = await storage.files("Tasks/.history");
    expect(logs).toHaveLength(1);
    const text = new TextDecoder().decode(await storage.read(`Tasks/.history/${logs[0].name}`));
    expect(
      text
        .trim()
        .split("\n")
        .map((line) => line.split(" ")[1]),
    ).toEqual(["created", "created"]);
  });

  it("keeps the Device ID on the Device, outside the Tasks folder", async () => {
    const storage = new MemoryStorage();
    await TaskFiles.open(core, storage);
    const id = new TextDecoder().decode(await storage.read("Device/device-id"));
    const again = await TaskFiles.open(core, storage);
    again.store.create("x", Planning.Active);
    await again.save();
    const [log] = await storage.files("Tasks/.history");
    expect(log.name.startsWith(`${id}-`)).toBe(true);
  });

  it("saves each change once, in the order it was made", async () => {
    const storage = new MemoryStorage();
    const files = await TaskFiles.open(core, storage);
    files.store.create("A", Planning.Backlog);
    const saving = files.save();
    files.store.plan(files.store.tasks(Planning.Backlog)[0].id, Planning.Active);
    await Promise.all([saving, files.save()]);
    const reopened = await TaskFiles.open(core, storage);
    expect(titles(reopened, Planning.Active)).toEqual(["A"]);
    expect(titles(reopened, Planning.Backlog)).toEqual([]);
  });
});
