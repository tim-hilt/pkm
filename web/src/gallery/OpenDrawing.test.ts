import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, type Editor, Tool, loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
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
