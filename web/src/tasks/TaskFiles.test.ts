import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, Closing, Planning, loadCore } from "../core/Core";
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
    const first = await TaskFiles.open(storage);
    first.store.create("Buy milk", Planning.Active, 4, []);
    await first.save();
    expect((await storage.files("Tasks")).map((file) => file.name)).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    const second = await TaskFiles.open(storage);
    expect(titles(second, Planning.Active)).toEqual(["Buy milk"]);
  });

  it("deletes the History old versions kept", async () => {
    const storage = new MemoryStorage();
    await storage.write("Tasks/.history/aaaaaaaaaaaa-2025-01.log", new TextEncoder().encode("x\n"));
    const files = await TaskFiles.open(storage);
    files.store.create("One", Planning.Active, 4, []);
    await files.save();
    expect(await storage.files("Tasks/.history")).toEqual([]);
    expect(await storage.files("Tasks")).toHaveLength(1);
  });

  it("reloads with what was changed and Closed while it read the files", async () => {
    const storage = new MemoryStorage();
    const files = await TaskFiles.open(storage);
    files.store.create("Closed meanwhile", Planning.Active, 4, []);
    await files.save();
    const [closed] = files.store.tasks(Planning.Active);

    const reloading = files.reload();
    files.store.create("Made meanwhile", Planning.Active, 4, []);
    void files.save();
    files.store.close(closed.id, Closing.Done);
    await reloading;

    expect(titles(files, Planning.Active)).toEqual(["Made meanwhile"]);
  });

  it("saves each change once, in the order it was made", async () => {
    const storage = new MemoryStorage();
    const files = await TaskFiles.open(storage);
    files.store.create("A", Planning.Backlog, 4, []);
    const saving = files.save();
    files.store.plan(files.store.tasks(Planning.Backlog)[0].id, Planning.Active);
    await Promise.all([saving, files.save()]);
    const reopened = await TaskFiles.open(storage);
    expect(titles(reopened, Planning.Active)).toEqual(["A"]);
    expect(titles(reopened, Planning.Backlog)).toEqual([]);
  });
});
