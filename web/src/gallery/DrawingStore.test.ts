import { beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryStorage } from "../storage/MemoryStorage";
import { DrawingStore, InvalidName, NameTaken, NotFound } from "./DrawingStore";

const bytes = (text: string) => new TextEncoder().encode(text);
const text = (data: Uint8Array) => new TextDecoder().decode(data);

let storage: MemoryStorage;
let store: DrawingStore;
beforeEach(() => {
  storage = new MemoryStorage();
  store = new DrawingStore(storage);
});

const names = async () => (await store.list()).map((entry) => entry.name);
const entry = async (name: string) => (await store.list()).find((e) => e.name === name)!;
const filesOnDisk = async () => (await storage.files("Drawings")).map((file) => file.name).sort();

describe("listing", () => {
  it("lists nothing for an empty store", async () => {
    expect(await store.list()).toEqual([]);
  });

  it("lists the Drawings last modified first", async () => {
    await store.save(bytes("a"), "a");
    await store.save(bytes("b"), "b");
    await store.save(bytes("c"), "c");
    expect(await names()).toEqual(["c", "b", "a"]);
  });

  it("moves a saved Drawing to the top", async () => {
    await store.save(bytes("a"), "a");
    await store.save(bytes("b"), "b");
    await store.save(bytes("a2"), "a");
    expect(await names()).toEqual(["a", "b"]);
  });

  it("lists only .excalidraw files, by their name without the extension", async () => {
    await store.save(bytes("a"), "a");
    await storage.write("Drawings/notes.txt", bytes("x"));
    await storage.write("Drawings/a.png", bytes("x"));
    expect(await names()).toEqual(["a"]);
  });
});

describe("new Drawings", () => {
  it("are named Untitled and the date and time, without colons", async () => {
    const name = await store.create(bytes("{}"), new Date(2026, 10, 3, 19, 27, 31));
    expect(name).toBe("Untitled 2026-11-03 19.27.31");
    expect(text(await store.open(name))).toBe("{}");
  });

  it("get distinct names when made in the same second", async () => {
    const date = new Date(2026, 10, 3, 19, 27, 31);
    const first = await store.create(bytes("1"), date);
    const second = await store.create(bytes("2"), date);
    const third = await store.create(bytes("3"), date);
    expect([first, second, third]).toEqual([
      "Untitled 2026-11-03 19.27.31",
      "Untitled 2026-11-03 19.27.31 2",
      "Untitled 2026-11-03 19.27.31 3",
    ]);
  });

  it("keep the name they are added under, trimmed", async () => {
    expect(await store.add(bytes("x"), "  plan ")).toBe("plan");
  });

  it("never replace another Drawing", async () => {
    await store.add(bytes("one"), "plan");
    expect(await store.add(bytes("two"), "plan")).toBe("plan 2");
    expect(text(await store.open("plan"))).toBe("one");
    expect(text(await store.open("plan 2"))).toBe("two");
  });
});

describe("opening and saving", () => {
  it("open with the bytes that were saved", async () => {
    const original = new Uint8Array([0, 255, 10, 13]);
    await store.save(original, "a");
    expect(await store.open("a")).toEqual(original);
  });

  it("replaces the earlier contents and leaves nothing else behind", async () => {
    for (let round = 0; round < 5; round++) await store.save(bytes("x".repeat(round)), "sketch");
    expect(await filesOnDisk()).toEqual(["sketch.excalidraw"]);
    expect(text(await store.open("sketch"))).toBe("xxxx");
  });

  it("fails to open a missing Drawing", async () => {
    await expect(store.open("missing")).rejects.toBeInstanceOf(NotFound);
  });

  it("rejects names that leave the folder or are empty", async () => {
    await expect(store.save(bytes(""), "../escape")).rejects.toBeInstanceOf(InvalidName);
    await expect(store.open("a/b")).rejects.toBeInstanceOf(InvalidName);
    await expect(store.save(bytes(""), "")).rejects.toBeInstanceOf(InvalidName);
    await expect(store.save(bytes(""), ".hidden")).rejects.toBeInstanceOf(InvalidName);
  });
});

describe("renaming", () => {
  it("changes the name and keeps the contents", async () => {
    await store.save(bytes("a"), "old");
    expect(await store.rename("old", "new")).toBe("new");
    expect(await names()).toEqual(["new"]);
    expect(text(await store.open("new"))).toBe("a");
  });

  it("trims surrounding whitespace", async () => {
    await store.save(bytes("a"), "old");
    expect(await store.rename("old", "  new\n")).toBe("new");
  });

  it("to a taken name fails and changes nothing", async () => {
    await store.save(bytes("a"), "a");
    await store.save(bytes("b"), "b");
    await expect(store.rename("a", "b")).rejects.toEqual(new NameTaken("b"));
    expect(text(await store.open("a"))).toBe("a");
    expect(text(await store.open("b"))).toBe("b");
  });

  it("to the same name changes nothing", async () => {
    await store.save(bytes("a"), "a");
    const before = await entry("a");
    expect(await store.rename("a", "a")).toBe("a");
    expect(await entry("a")).toEqual(before);
  });

  it("to another case of the name works", async () => {
    await store.save(bytes("a"), "plan");
    await store.rename("plan", "Plan");
    expect(await names()).toEqual(["Plan"]);
  });

  it("to a name that cannot be used fails", async () => {
    await store.save(bytes("a"), "a");
    await expect(store.rename("a", " ")).rejects.toBeInstanceOf(InvalidName);
    await expect(store.rename("a", "x/y")).rejects.toBeInstanceOf(InvalidName);
    expect(await names()).toEqual(["a"]);
  });

  it("a missing Drawing fails", async () => {
    await expect(store.rename("missing", "other")).rejects.toBeInstanceOf(NotFound);
  });
});

describe("duplicating", () => {
  it("makes a copy named '<name> copy', then '<name> copy 2'", async () => {
    await store.save(bytes("a"), "plan");
    expect(await store.duplicate("plan")).toBe("plan copy");
    expect(await store.duplicate("plan")).toBe("plan copy 2");
    expect(text(await store.open("plan copy 2"))).toBe("a");
  });

  it("makes the copy the newest Drawing", async () => {
    await store.save(bytes("a"), "plan");
    await store.save(bytes("b"), "other");
    await store.duplicate("plan");
    expect((await names())[0]).toBe("plan copy");
  });

  it("fails for a missing Drawing", async () => {
    await expect(store.duplicate("missing")).rejects.toBeInstanceOf(NotFound);
  });
});

describe("deleting", () => {
  it("removes the file", async () => {
    await store.save(bytes("a"), "a");
    await store.delete("a");
    expect(await names()).toEqual([]);
    await expect(store.open("a")).rejects.toBeInstanceOf(NotFound);
  });

  it("removes the caches of that Drawing only", async () => {
    await store.save(bytes("a"), "a");
    await store.save(bytes("b"), "b");
    await store.saveViewport({ originX: 1, originY: 2, zoom: 3 }, "a");
    await store.saveViewport({ originX: 4, originY: 5, zoom: 6 }, "b");
    await store.delete("a");
    expect(await store.viewport("a")).toBeUndefined();
    expect(await store.viewport("b")).toEqual({ originX: 4, originY: 5, zoom: 6 });
  });

  it("fails for a missing Drawing", async () => {
    await expect(store.delete("missing")).rejects.toBeInstanceOf(NotFound);
  });

  it("lets a deleted name be used again without the old caches", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport({ originX: 1, originY: 2, zoom: 3 }, "a");
    await store.delete("a");
    await store.save(bytes("a"), "a");
    expect(await store.viewport("a")).toBeUndefined();
  });

  it("is followed by the removal of caches that belong to no Drawing", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport({ originX: 1, originY: 2, zoom: 3 }, "a");
    await storage.write("Caches/gone/viewport.json", bytes("{}"));
    await store.removeOrphanedCaches();
    expect(await storage.folders("Caches")).toEqual(["a"]);
  });
});

describe("thumbnails", () => {
  it("are none until saved", async () => {
    await store.save(bytes("a"), "a");
    expect(await store.thumbnail(await entry("a"))).toBeUndefined();
  });

  it("come back while the Drawing is unchanged", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("png"), await entry("a"));
    expect(text((await store.thumbnail(await entry("a")))!)).toBe("png");
  });

  it("are kept per appearance", async () => {
    await store.save(bytes("a"), "a");
    const e = await entry("a");
    await store.saveThumbnail(bytes("light"), e);
    await store.saveThumbnail(bytes("dark"), e, true);
    expect(text((await store.thumbnail(e))!)).toBe("light");
    expect(text((await store.thumbnail(e, true))!)).toBe("dark");
  });

  it("are stale once the Drawing changed", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("png"), await entry("a"));
    await store.save(bytes("changed"), "a");
    expect(await store.thumbnail(await entry("a"))).toBeUndefined();
  });

  it("are replaced by a new one", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("one"), await entry("a"));
    await store.saveThumbnail(bytes("two"), await entry("a"));
    expect(text((await store.thumbnail(await entry("a")))!)).toBe("two");
    expect(await storage.files("Caches/a")).toHaveLength(1);
  });

  it("live outside the Drawings' folder and are no change of the Drawing", async () => {
    await store.save(bytes("a"), "a");
    const before = await entry("a");
    await store.saveThumbnail(bytes("png"), before);
    expect(await filesOnDisk()).toEqual(["a.excalidraw"]);
    expect(await entry("a")).toEqual(before);
  });

  it("follow a renamed Drawing", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("png"), await entry("a"));
    await store.saveThumbnail(bytes("dark"), await entry("a"), true);
    await store.rename("a", "b");
    expect(text((await store.thumbnail(await entry("b")))!)).toBe("png");
    expect(text((await store.thumbnail(await entry("b"), true))!)).toBe("dark");
    expect(await storage.folders("Caches")).toEqual(["b"]);
  });

  it("are not shared with a duplicate", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("png"), await entry("a"));
    const copy = await store.duplicate("a");
    expect(await store.thumbnail(await entry(copy))).toBeUndefined();
  });
});

describe("viewports", () => {
  const viewport = { originX: -120.5, originY: 40, zoom: 2.5 };

  it("are none until saved", async () => {
    await store.save(bytes("a"), "a");
    expect(await store.viewport("a")).toBeUndefined();
  });

  it("come back as they were, the last one winning", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport(viewport, "a");
    expect(await store.viewport("a")).toEqual(viewport);
    await store.saveViewport({ originX: 1, originY: 2, zoom: 0.5 }, "a");
    expect(await store.viewport("a")).toEqual({ originX: 1, originY: 2, zoom: 0.5 });
  });

  it("that cannot be read are none", async () => {
    await store.save(bytes("a"), "a");
    await storage.write("Caches/a/viewport.json", bytes("not json"));
    expect(await store.viewport("a")).toBeUndefined();
    await storage.write("Caches/a/viewport.json", bytes('{"originX":1}'));
    expect(await store.viewport("a")).toBeUndefined();
  });

  it("are no change of the Drawing and stay out of its file", async () => {
    await store.save(bytes("a"), "a");
    const before = await entry("a");
    await store.saveViewport(viewport, "a");
    expect(await entry("a")).toEqual(before);
    expect(await filesOnDisk()).toEqual(["a.excalidraw"]);
    expect(text(await store.open("a"))).toBe("a");
  });

  it("do not get in the way of a thumbnail", async () => {
    await store.save(bytes("a"), "a");
    await store.saveThumbnail(bytes("png"), await entry("a"));
    await store.saveViewport(viewport, "a");
    expect(text((await store.thumbnail(await entry("a")))!)).toBe("png");
    expect(await store.viewport("a")).toEqual(viewport);
  });

  it("follow a renamed Drawing", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport(viewport, "a");
    await store.rename("a", "b");
    expect(await store.viewport("b")).toEqual(viewport);
    expect(await store.viewport("a")).toBeUndefined();
  });

  it("are not shared with a duplicate", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport(viewport, "a");
    expect(await store.viewport(await store.duplicate("a"))).toBeUndefined();
  });

  it("cannot be saved for a name that is no Drawing's", async () => {
    await expect(store.saveViewport(viewport, "../a")).rejects.toBeInstanceOf(InvalidName);
    expect(await store.viewport("../a")).toBeUndefined();
  });
});

describe("a Drawing under the name of one that was removed behind the store's back", () => {
  it("starts without its caches", async () => {
    await store.save(bytes("a"), "a");
    await store.saveViewport({ originX: 1, originY: 2, zoom: 3 }, "a");
    await storage.remove("Drawings/a.excalidraw");
    await store.add(bytes("new"), "a");
    expect(await store.viewport("a")).toBeUndefined();
  });
});

describe("saving", () => {
  it("tells whoever listens after every write of a Drawing, and only then", async () => {
    const saved = vi.fn();
    const listened = new DrawingStore(storage, saved);
    await listened.add(bytes("a"), "a");
    await listened.save(bytes("b"), "a");
    await listened.saveViewport({ originX: 0, originY: 0, zoom: 1 }, "a");
    await listened.rename("a", "b");
    expect(saved).toHaveBeenCalledTimes(2);
  });
});
