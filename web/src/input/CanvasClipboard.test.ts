import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, loadCore } from "../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { CanvasClipboard, type SystemClipboard } from "./CanvasClipboard";

const generated = new URL("../../generated/", import.meta.url);
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const rectangle = new TextEncoder().encode(
  `{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,"backgroundColor":"#ffc9c9","roundness":null}]}`,
);

/** An editor with the rectangle, selected by a tap. */
function withSelection(): Editor {
  const editor = core.makeEditor();
  editor.setViewSize(800, 600);
  editor.load(rectangle);
  drag(editor, [[150, 125]]);
  return editor;
}

/** An editor with nothing in it. */
function empty(): Editor {
  const editor = core.makeEditor();
  editor.setViewSize(800, 600);
  return editor;
}

/** The system clipboard as a fake: what was written, and what it holds to read. */
class FakeClipboard implements SystemClipboard {
  text: string | undefined;
  held: { type: string; data: Uint8Array | string }[] = [];
  async writeText(text: string) {
    this.text = text;
    this.held = [{ type: "text/plain", data: text }];
  }
  async read() {
    return [
      {
        types: this.held.map((h) => h.type),
        getType: async (type: string) => {
          const h = this.held.find((x) => x.type === type)!;
          return new Blob([h.data as BlobPart], { type });
        },
      },
    ];
  }
}

function setup(editor: Editor, system = new FakeClipboard()) {
  const inserted: { sources: Uint8Array[]; at?: { x: number; y: number } }[] = [];
  let changes = 0;
  const clipboard = new CanvasClipboard(editor, system, {
    images: (sources, at) => inserted.push({ sources, at }),
    changed: () => changes++,
  });
  return { clipboard, system, inserted, changes: () => changes };
}

describe("copying and pasting Elements", () => {
  it("copies the Selection as Excalidraw clipboard JSON, which pastes into another Canvas", async () => {
    const source = setup(withSelection());
    await source.clipboard.copy();
    expect(source.system.text).toContain(`"type":"excalidraw/clipboard"`);

    const target = empty();
    const pasting = setup(target, source.system);
    await pasting.clipboard.paste();
    expect(target.elementCount()).toBe(1);
    expect(target.selection()).toHaveLength(1);
    expect(target.canUndo()).toBe(true);
    expect(pasting.changes()).toBe(1);
  });

  it("copies nothing with nothing selected", async () => {
    const editor = empty();
    editor.load(rectangle);
    const { clipboard, system } = setup(editor);
    await clipboard.copy();
    expect(system.text).toBeUndefined();
  });

  it("cuts: the text is written and the Selection deleted, undoably", async () => {
    const editor = withSelection();
    const { clipboard, system, changes } = setup(editor);
    await clipboard.cut();
    expect(system.text).toContain("excalidraw/clipboard");
    expect(editor.selection()).toHaveLength(0);
    expect(editor.elementCount()).toBe(0);
    expect(changes()).toBe(1);
    editor.undo();
    expect(editor.selection()).toHaveLength(1);
  });

  it("does not paste text that is not clipboard JSON", async () => {
    const system = new FakeClipboard();
    await system.writeText("just some words");
    const editor = empty();
    const { clipboard, inserted, changes } = setup(editor, system);
    await clipboard.paste();
    expect(editor.elementCount()).toBe(0);
    expect(inserted).toEqual([]);
    expect(changes()).toBe(0);
  });

  it("places a paste around the given point", async () => {
    const source = setup(withSelection());
    await source.clipboard.copy();
    const target = empty();
    await setup(target, source.system).clipboard.paste({ x: 100, y: 80 });
    const saved = new TextDecoder().decode(target.save());
    expect(saved).toMatch(/"x": 50\b/);
    expect(saved).toMatch(/"y": 55\b/);
  });

  it("copies nothing and does not fail where the clipboard refuses", async () => {
    const system = new FakeClipboard();
    system.writeText = () => Promise.reject(new DOMException("denied", "NotAllowedError"));
    system.read = () => Promise.reject(new DOMException("denied", "NotAllowedError"));
    const { clipboard } = setup(withSelection(), system);
    await expect(clipboard.copy()).resolves.toBeUndefined();
    await expect(clipboard.paste()).resolves.toBeUndefined();
  });
});

describe("pasting an image", () => {
  it("hands the image's bytes to the image insertion", async () => {
    const system = new FakeClipboard();
    system.held = [{ type: "image/png", data: new Uint8Array([1, 2, 3]) }];
    const { clipboard, inserted } = setup(empty(), system);
    await clipboard.paste({ x: 5, y: 6 });
    expect(inserted).toHaveLength(1);
    expect([...inserted[0].sources[0]]).toEqual([1, 2, 3]);
    expect(inserted[0].at).toEqual({ x: 5, y: 6 });
  });

  it("prefers Elements to the image they come with", async () => {
    const source = setup(withSelection());
    await source.clipboard.copy();
    source.system.held.push({ type: "image/png", data: new Uint8Array([1]) });
    const target = empty();
    const { clipboard, inserted } = setup(target, source.system);
    await clipboard.paste();
    expect(target.elementCount()).toBe(1);
    expect(inserted).toEqual([]);
  });
});
