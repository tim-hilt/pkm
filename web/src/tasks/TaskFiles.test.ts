import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, Closing, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "./TaskFiles";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const titles = (files: TaskFiles) => {
  const rows = files.store.tasks();
  const found = rows.map((row) => row.title);
  rows.forEach((row) => row.free());
  return found;
};

describe("the Tasks in storage", () => {
  it("keeps a created Task in the Tasks folder, and finds it again", async () => {
    const storage = new MemoryStorage();
    const first = await TaskFiles.open(storage);
    first.store.create("Buy milk", 4, []);
    await first.save();
    expect((await storage.files("Tasks")).map((file) => file.name).sort()).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    const second = await TaskFiles.open(storage);
    expect(titles(second)).toEqual(["Buy milk"]);
  });

  it("reloads with what was changed and Closed while it read the files", async () => {
    const storage = new MemoryStorage();
    const files = await TaskFiles.open(storage);
    files.store.create("Closed meanwhile", 4, []);
    await files.save();
    const [closed] = files.store.tasks();

    const reloading = files.reload();
    files.store.create("Made meanwhile", 4, []);
    void files.save();
    files.store.close(closed.id, Closing.Done);
    await reloading;

    expect(titles(files)).toEqual(["Made meanwhile"]);
  });

  it("saves each change once, in the order it was made", async () => {
    const storage = new MemoryStorage();
    const files = await TaskFiles.open(storage);
    files.store.create("A", 4, []);
    const saving = files.save();
    files.store.addTag(files.store.tasks()[0].id, "x");
    await Promise.all([saving, files.save()]);
    const reopened = await TaskFiles.open(storage);
    const [row] = reopened.store.tasks();
    expect(row.tags().map((tag) => tag.name)).toEqual(["x"]);
    row.free();
  });
});
