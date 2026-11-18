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
      ".lists-migrated",
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    const second = await TaskFiles.open(storage);
    expect(titles(second)).toEqual(["Buy milk"]);
  });

  const encode = (text: string) => new TextEncoder().encode(text);
  const decode = (bytes?: Uint8Array) => new TextDecoder().decode(bytes);
  const legacy = (id: string, state: string) =>
    encode(
      `---\nid: ${id}\ntitle: ${id}\npriority: 4\nstate: ${state}\nposition: 1\nupdated: 2000-01-01T00:00:00Z\n---\n`,
    );
  const names = async (storage: MemoryStorage, folder: string) =>
    (await storage.files(folder)).map((f) => f.name).sort();

  it("migrates the files of an old version once, on open", async () => {
    const storage = new MemoryStorage();
    await storage.write("Tasks/aaaaaaaaaaaa.md", legacy("aaaaaaaaaaaa", "backlog"));
    await storage.write("Tasks/bbbbbbbbbbbb.md", legacy("bbbbbbbbbbbb", "done"));
    const first = await TaskFiles.open(storage);

    expect(await names(storage, "Tasks")).toEqual([".lists-migrated", "aaaaaaaaaaaa.md"]);
    expect(await names(storage, "Filters")).toHaveLength(3);
    const text = decode(await storage.read("Tasks/aaaaaaaaaaaa.md"));
    expect(text).toContain("- backlog");
    expect(text).not.toContain("state:");
    expect(titles(first)).toEqual(["aaaaaaaaaaaa"]);

    // A Filter Deleted afterwards stays deleted: the Device remembers it migrated.
    await storage.remove(`Filters/${(await names(storage, "Filters"))[0]}`);
    await TaskFiles.open(storage);
    expect(await names(storage, "Filters")).toHaveLength(2);
  });

  it("takes the creation times from the History, then deletes it", async () => {
    const storage = new MemoryStorage();
    await storage.write("Tasks/aaaaaaaaaaaa.md", legacy("aaaaaaaaaaaa", "active"));
    const log = "Tasks/.history/pppppppppppp-2023-11.log";
    await storage.write(log, encode("2023-11-14T22:13:20Z created aaaaaaaaaaaa\n"));
    await TaskFiles.open(storage);
    expect(await storage.files("Tasks/.history")).toEqual([]);
    expect(decode(await storage.read("Tasks/aaaaaaaaaaaa.md"))).toContain(
      "created: 2023-11-14T22:13:20Z",
    );
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
