import { readFileSync } from "node:fs";
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, Tool, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { DrawingStore } from "./DrawingStore";
import { Gallery } from "./Gallery";
import { Thumbnails } from "./Thumbnails";

const generated = new URL("../../generated/", import.meta.url);
const sample = readFileSync(new URL("sample.excalidraw", generated));
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);

let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const bytes = (text: string) => new TextEncoder().encode(text);
const emptyDrawing = () => {
  const editor = core.makeEditor();
  const saved = editor.save();
  editor.free();
  return saved;
};

let storage: MemoryStorage;
let store: DrawingStore;
beforeEach(() => {
  storage = new MemoryStorage();
  store = new DrawingStore(storage);
});

const start = () =>
  Gallery.start(store, core, new Thumbnails(store, { render: async () => undefined }));
const names = (gallery: Gallery) => gallery.drawings.map((entry) => entry.name);

describe("the Gallery", () => {
  it("starts with the stored Drawings and nothing open", async () => {
    await store.save(emptyDrawing(), "one");
    const gallery = await start();
    expect(names(gallery)).toEqual(["one"]);
    expect(gallery.open).toBeUndefined();
  });

  it("starts by removing the caches of Drawings that are gone", async () => {
    await storage.write("Caches/deleted/viewport.json", bytes("{}"));
    await start();
    expect(await storage.folders("Caches")).toEqual([]);
  });

  it("tells its listener about every change", async () => {
    const gallery = await start();
    const changed = vi.fn();
    gallery.onChange = changed;
    await gallery.createDrawing();
    expect(changed).toHaveBeenCalled();
  });
});

describe("opening and leaving", () => {
  it("creating a Drawing opens an empty one", async () => {
    const gallery = await start();
    await gallery.createDrawing();
    expect(gallery.open?.name.startsWith("Untitled ")).toBe(true);
    expect(gallery.open?.editor.elementCount()).toBe(0);
    expect(gallery.failure).toBeUndefined();
  });

  it("going back saves the Drawing and lists it", async () => {
    const gallery = await start();
    await gallery.createDrawing();
    const drawing = gallery.open!;
    drawing.editor.setTool(Tool.Rectangle);
    drag(drawing.editor, [
      [10, 10],
      [90, 90],
    ]);
    drawing.editorDidChange();

    await gallery.close();

    expect(gallery.open).toBeUndefined();
    expect(names(gallery)).toEqual([drawing.name]);
    const saved = core.makeEditor();
    saved.load(await store.open(drawing.name));
    expect(saved.elementCount()).toBe(1);
    saved.free();
  });

  it("going back from an empty Drawing removes it from the Gallery", async () => {
    const gallery = await start();
    await gallery.createDrawing();
    await gallery.close();
    expect(gallery.open).toBeUndefined();
    expect(gallery.drawings).toEqual([]);
  });

  it("opening a listed Drawing shows it", async () => {
    await store.save(sample, "sample");
    const gallery = await start();
    await gallery.openDrawing("sample");
    expect(gallery.open?.name).toBe("sample");
    expect(gallery.open?.editor.elementCount()).toBe(8);
  });

  it("a Drawing that cannot be opened stays in the Gallery with a failure", async () => {
    await store.save(bytes("not json"), "broken");
    const gallery = await start();
    await gallery.openDrawing("broken");
    expect(gallery.open).toBeUndefined();
    expect(gallery.failure).toBeDefined();
    expect(await store.open("broken")).toEqual(bytes("not json"));
  });
});

describe("renaming, duplicating and deleting", () => {
  it("a renamed Drawing is listed under its new name", async () => {
    await store.save(emptyDrawing(), "old");
    const gallery = await start();
    await gallery.renameDrawing("old", "new");
    expect(names(gallery)).toEqual(["new"]);
    expect(gallery.failure).toBeUndefined();
  });

  it("renaming to a taken name tells the user and changes nothing", async () => {
    await store.save(bytes("a"), "a");
    await store.save(bytes("b"), "b");
    const gallery = await start();
    await gallery.renameDrawing("a", "b");
    expect(gallery.failure).toContain("“b”");
    expect(new Set(names(gallery))).toEqual(new Set(["a", "b"]));
    expect(await store.open("b")).toEqual(bytes("b"));
  });

  it("renaming to an empty name tells the user", async () => {
    await store.save(emptyDrawing(), "a");
    const gallery = await start();
    await gallery.renameDrawing("a", " ");
    expect(gallery.failure).toBeDefined();
    expect(names(gallery)).toEqual(["a"]);
  });

  it("a duplicate is listed next to the original, and the Gallery stays", async () => {
    await store.save(emptyDrawing(), "plan");
    const gallery = await start();
    await gallery.duplicateDrawing("plan");
    expect(new Set(names(gallery))).toEqual(new Set(["plan", "plan copy"]));
    expect(gallery.open).toBeUndefined();
    expect(gallery.failure).toBeUndefined();
  });

  it("a deleted Drawing is gone", async () => {
    await store.save(emptyDrawing(), "a");
    await store.save(emptyDrawing(), "b");
    const gallery = await start();
    await gallery.deleteDrawing("a");
    expect(names(gallery)).toEqual(["b"]);
    await expect(store.open("a")).rejects.toThrow();
  });

  it("a failed change is cleared by the next successful one", async () => {
    await store.save(emptyDrawing(), "a");
    const gallery = await start();
    await gallery.deleteDrawing("missing");
    expect(gallery.failure).toBeDefined();
    await gallery.duplicateDrawing("a");
    expect(gallery.failure).toBeUndefined();
  });

  it("a failure can be dismissed", async () => {
    const gallery = await start();
    await gallery.deleteDrawing("missing");
    gallery.dismissFailure();
    expect(gallery.failure).toBeUndefined();
  });

  describe("importing .excalidraw files", () => {
    const file = (name: string, bytes: Uint8Array = emptyDrawing()) => ({ name, bytes });

    it("adds a Drawing named like the file, without the extension", async () => {
      const gallery = await start();
      await gallery.importDrawings([file("My plan.excalidraw", new Uint8Array(sample))]);
      expect(names(gallery)).toEqual(["My plan"]);
      expect(await store.open("My plan")).toEqual(new Uint8Array(sample));
      expect(gallery.failure).toBeUndefined();
    });

    it("gives a name that is taken a distinct one and never replaces the Drawing", async () => {
      const original = bytes('{"type":"excalidraw","version":2,"elements":[]}');
      await store.save(original, "plan");
      const gallery = await start();
      await gallery.importDrawings([file("plan.excalidraw"), file("PLAN.EXCALIDRAW")]);
      expect(names(gallery).sort()).toEqual(["PLAN", "plan", "plan 2"]);
      expect(await store.open("plan")).toEqual(original);
    });

    it("reports a file that cannot be read, still adding the others", async () => {
      const gallery = await start();
      const changed = vi.fn();
      gallery.onChange = changed;
      await gallery.importDrawings([
        file("broken.excalidraw", bytes("not json")),
        file("fine.excalidraw"),
      ]);
      expect(names(gallery)).toEqual(["fine"]);
      expect(gallery.failure).toContain("“broken.excalidraw”");
      expect(gallery.failure).not.toContain("fine");
      expect(changed).toHaveBeenCalled();
    });

    it("clears an earlier failure on a good import", async () => {
      const gallery = await start();
      await gallery.importDrawings([file("x.excalidraw", bytes("{"))]);
      await gallery.importDrawings([file("y.excalidraw")]);
      expect(gallery.failure).toBeUndefined();
    });

    it("names a file that is only an extension", async () => {
      const gallery = await start();
      await gallery.importDrawings([file(".excalidraw")]);
      expect(names(gallery)).toEqual(["Imported"]);
    });
  });
});
