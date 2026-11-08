import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, type Editor, Tool, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { Excalifont } from "../text/Excalifont";
import { drag, tap } from "../testing/Gestures";
import { DrawingStore } from "./DrawingStore";
import { OpenDrawing } from "./OpenDrawing";

const generated = new URL("../../generated/", import.meta.url);
const sample = readFileSync(new URL("sample.excalidraw", generated));
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);

let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const delay = 150;
let store: DrawingStore;
beforeEach(() => {
  vi.useFakeTimers();
  store = new DrawingStore(new MemoryStorage());
});
afterEach(() => vi.useRealTimers());

const emptyDrawing = () => {
  const editor = core.makeEditor();
  const bytes = editor.save();
  editor.free();
  return bytes;
};

const openNew = async () =>
  OpenDrawing.open(await store.create(emptyDrawing()), store, core, { autosaveDelay: delay });

/** Draws a rectangle and tells the Drawing its editor changed, as the Canvas does after input. */
function drawRectangle(drawing: OpenDrawing, to = 100) {
  drawing.editor.setTool(Tool.Rectangle);
  drag(drawing.editor, [
    [10, 10],
    [to, 60],
  ]);
  drawing.editorDidChange();
}

const savedElementCount = async (name: string) => {
  const editor = core.makeEditor();
  editor.load(await store.open(name));
  const count = editor.elementCount();
  editor.free();
  return count;
};

const wait = (ms: number) => vi.advanceTimersByTimeAsync(ms);
const plain = (editor: Editor) => new TextDecoder().decode(editor.save());

describe("opening", () => {
  it("opens the stored Drawing", async () => {
    const name = await store.create(sample);
    const drawing = await OpenDrawing.open(name, store, core);
    expect(drawing.name).toBe(name);
    expect(drawing.editor.elementCount()).toBe(8);
  });

  it("does not open a Drawing that cannot be read, and leaves it alone", async () => {
    await store.save(new TextEncoder().encode("not json"), "broken");
    await expect(OpenDrawing.open("broken", store, core)).rejects.toThrow();
    expect(new TextDecoder().decode(await store.open("broken"))).toBe("not json");
  });
});

describe("autosave", () => {
  it("saves a change after the delay, not before", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    expect(await savedElementCount(drawing.name)).toBe(0);
    await wait(delay * 4);
    expect(await savedElementCount(drawing.name)).toBe(1);
  });

  it("restarts the delay with each change", async () => {
    const drawing = await openNew();
    const before = await store.open(drawing.name);
    drawRectangle(drawing);
    await wait((delay * 2) / 3);
    drawRectangle(drawing, 200);
    await wait((delay * 2) / 3);
    // 4/3 of the delay since the first change, but only 2/3 since the last.
    expect(await store.open(drawing.name)).toEqual(before);
    await wait(delay * 4);
    expect(await savedElementCount(drawing.name)).toBe(2);
  });

  it("saves nothing without a change", async () => {
    const drawing = await openNew();
    await store.save(new TextEncoder().encode("changed elsewhere"), drawing.name);
    drawing.editorDidChange();
    await wait(delay * 4);
    expect(new TextDecoder().decode(await store.open(drawing.name))).toBe("changed elsewhere");
  });

  it("saves at once when asked", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    expect(await savedElementCount(drawing.name)).toBe(1);
  });

  it("does not repeat the delayed save after a save at once", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    await store.save(new TextEncoder().encode("changed elsewhere"), drawing.name);
    await wait(delay * 4);
    expect(new TextDecoder().decode(await store.open(drawing.name))).toBe("changed elsewhere");
  });

  it("leaves the file alone when saving at once without changes", async () => {
    const drawing = await openNew();
    await store.save(new TextEncoder().encode("changed elsewhere"), drawing.name);
    await drawing.saveNow();
    expect(new TextDecoder().decode(await store.open(drawing.name))).toBe("changed elsewhere");
  });

  it("reopens as it was saved", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const reopened = await OpenDrawing.open(drawing.name, store, core);
    expect(reopened.editor.elementCount()).toBe(1);
    expect(plain(reopened.editor)).toBe(plain(drawing.editor));
  });

  it("keeps the changes unsaved when a save fails, and tries again", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    const save = vi.spyOn(store, "save").mockRejectedValueOnce(new Error("disk full"));
    await drawing.saveNow();
    expect(await savedElementCount(drawing.name)).toBe(0);
    save.mockRestore();
    await drawing.saveNow();
    expect(await savedElementCount(drawing.name)).toBe(1);
  });
});

describe("leaving", () => {
  it("removes the file of an empty new Drawing", async () => {
    const drawing = await openNew();
    await drawing.close();
    expect(await store.list()).toEqual([]);
  });

  it("removes the file of a Drawing the user emptied, and nothing brings it back", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    drawing.editor.undo();
    expect(drawing.editor.elementCount()).toBe(0);
    drawing.editorDidChange();

    await drawing.close();
    await drawing.saveNow();
    await wait(delay * 4);

    expect(await store.list()).toEqual([]);
  });

  it("keeps the file of a Drawing with Elements, saved", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.close();
    expect(await savedElementCount(drawing.name)).toBe(1);
  });

  it("commits a text being edited, as typed", async () => {
    const drawing = await openNew();
    drawing.editor.setViewSize(800, 600);
    drawing.editor.setTool(Tool.Text);
    tap(drawing.editor, 300, 200);
    drawing.editor.setEditingText("Hi");
    drawing.editorDidChange();

    await drawing.close();

    expect(drawing.editor.textEditor()).toBeUndefined();
    expect(drawing.editor.canUndo()).toBe(true);
    expect(await savedElementCount(drawing.name)).toBe(1);
  });

  it("does not save a pending change again after leaving", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.close();
    await store.save(new TextEncoder().encode("later"), drawing.name);
    await wait(delay * 4);
    expect(new TextDecoder().decode(await store.open(drawing.name))).toBe("later");
  });
});

describe("the viewport", () => {
  const scrollAndZoom = (drawing: OpenDrawing) => {
    drawing.editor.setViewSize(400, 300);
    drawing.editor.scroll({
      x: 0,
      y: 0,
      dx: 30,
      dy: 70,
      modifiers: { shift: false, ctrl: false, alt: false, meta: false },
    });
    drawing.editor.zoomIn();
    drawing.editor.zoomIn();
  };
  const defaultViewport = () => {
    const editor = core.makeEditor();
    const viewport = editor.viewport();
    editor.free();
    return viewport;
  };
  const newName = () => store.create(emptyDrawing());

  it("is the default for a Drawing that was never left", async () => {
    const drawing = await OpenDrawing.open(await newName(), store, core);
    expect(drawing.editor.viewport()).toEqual(defaultViewport());
  });

  it("is the one the Drawing was left at", async () => {
    const name = await newName();
    const drawing = await OpenDrawing.open(name, store, core);
    scrollAndZoom(drawing);
    const left = drawing.editor.viewport();
    expect(left).not.toEqual(defaultViewport());

    await drawing.saveNow();

    expect((await OpenDrawing.open(name, store, core)).editor.viewport()).toEqual(left);
  });

  it("is kept per Drawing", async () => {
    const [a, b] = [await newName(), await newName()];
    const first = await OpenDrawing.open(a, store, core);
    scrollAndZoom(first);
    await first.saveNow();
    expect((await OpenDrawing.open(b, store, core)).editor.viewport()).toEqual(defaultViewport());
  });

  it("is the one of the last leave", async () => {
    const name = await newName();
    const first = await OpenDrawing.open(name, store, core);
    scrollAndZoom(first);
    await first.saveNow();
    const second = await OpenDrawing.open(name, store, core);
    second.editor.resetZoom();
    const left = second.editor.viewport();
    await second.saveNow();
    expect((await OpenDrawing.open(name, store, core)).editor.viewport()).toEqual(left);
  });

  it("is kept without any change to the Scene, and never written into the file", async () => {
    const name = await newName();
    const bytes = await store.open(name);
    const before = (await store.list()).find((e) => e.name === name);
    const drawing = await OpenDrawing.open(name, store, core);
    scrollAndZoom(drawing);

    await drawing.saveNow();

    expect((await store.list()).find((e) => e.name === name)).toEqual(before);
    expect(await store.open(name)).toEqual(bytes);

    drawRectangle(drawing);
    await drawing.saveNow();
    const saved = new TextDecoder().decode(await store.open(name));
    expect(saved).toContain("rectangle");
    expect(saved.toLowerCase()).not.toContain("zoom");
    expect(saved).not.toContain("scrollX");
  });
});

describe("Synced changes", () => {
  /** What another Device made of `name`'s Drawing: one more rectangle. */
  const withRemoteRectangle = async (name: string) => {
    const other = core.makeEditor();
    other.load(await store.open(name));
    other.setTool(Tool.Rectangle);
    drag(other, [
      [200, 200],
      [260, 240],
    ]);
    const bytes = other.save();
    other.free();
    return bytes;
  };

  /** Sync rewriting `file` as it does: only if it still holds what it read. */
  const syncWrites = async (name: string, bytes: Uint8Array) =>
    store.writeIfHolding(`${name}.excalidraw`, bytes, await store.open(name));

  it("shows a change from another Device without an undo step, and undo still undoes the user's own", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const remote = await withRemoteRectangle(drawing.name);
    expect(await syncWrites(drawing.name, remote)).toBe(true);

    await drawing.syncDidChange([drawing.name]);
    expect(drawing.editor.elementCount()).toBe(2);

    drawing.editor.undo();
    expect(drawing.editor.elementCount()).toBe(1);
    expect(drawing.editor.canUndo()).toBe(false);
  });

  it("takes the change without saving it again when nothing of the user's was unsaved", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const remote = await withRemoteRectangle(drawing.name);
    await syncWrites(drawing.name, remote);
    await drawing.syncDidChange([drawing.name]);
    const before = (await store.list())[0].modified;

    await wait(delay * 3);
    expect((await store.list())[0].modified).toBe(before);
    expect(await savedElementCount(drawing.name)).toBe(2);
  });

  it("merges the change into what the user did not save yet, and the next save writes both", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const remote = await withRemoteRectangle(drawing.name);
    await syncWrites(drawing.name, remote);
    drawing.editor.setTool(Tool.Rectangle);
    drag(drawing.editor, [
      [300, 300],
      [360, 340],
    ]);
    drawing.editorDidChange();

    await drawing.syncDidChange([drawing.name]);
    expect(drawing.editor.elementCount()).toBe(3);
    await wait(delay);
    expect(await savedElementCount(drawing.name)).toBe(3);
  });

  it("keeps the viewport", async () => {
    const drawing = await openNew();
    drawing.editor.setViewport({ originX: 40, originY: 30, zoom: 2 });
    drawRectangle(drawing);
    await drawing.saveNow();
    await syncWrites(drawing.name, await withRemoteRectangle(drawing.name));
    await drawing.syncDidChange([drawing.name]);
    expect(drawing.editor.viewport()).toEqual({ originX: 40, originY: 30, zoom: 2 });
  });

  it("leaves a file that is not a Drawing alone", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    await syncWrites(drawing.name, new TextEncoder().encode("nope"));
    await drawing.syncDidChange([drawing.name]);
    expect(drawing.editor.elementCount()).toBe(1);
  });

  it("follows a rename by Sync and goes on saving under the new name", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const old = drawing.name;
    const bytes = await store.open(old);
    expect(await store.renameIfHolding(`${old}.excalidraw`, "Renamed.excalidraw", bytes)).toBe(
      true,
    );

    await drawing.syncDidChange([old, "Renamed"]);
    expect(drawing.name).toBe("Renamed");

    drawRectangle(drawing, 200);
    await drawing.saveNow();
    expect(await savedElementCount("Renamed")).toBe(2);
    expect((await store.list()).map((e) => e.name)).toEqual(["Renamed"]);
  });

  it("follows a rename and takes the changes that came with it", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const old = drawing.name;
    const remote = await withRemoteRectangle(old);
    await store.renameIfHolding(`${old}.excalidraw`, "Renamed.excalidraw", await store.open(old));
    await syncWrites("Renamed", remote);

    await drawing.syncDidChange([old, "Renamed"]);
    expect(drawing.name).toBe("Renamed");
    expect(drawing.editor.elementCount()).toBe(2);
  });

  it("stays open when Sync deletes it, and its next save writes it again", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.saveNow();
    const name = drawing.name;
    expect(await store.deleteIfHolding(`${name}.excalidraw`, await store.open(name))).toBe(true);

    await drawing.syncDidChange([name]);
    expect(drawing.name).toBe(name);
    expect(drawing.editor.elementCount()).toBe(1);
    expect(await store.list()).toEqual([]);

    drawRectangle(drawing, 200);
    await wait(delay);
    expect(await savedElementCount(name)).toBe(2);
  });

  it("does nothing after the Drawing was left", async () => {
    const drawing = await openNew();
    drawRectangle(drawing);
    await drawing.close();
    await drawing.syncDidChange([drawing.name]);
    expect(await savedElementCount(drawing.name)).toBe(1);
  });
});
