import { readFileSync } from "node:fs";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { type Core, type Editor, loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { CanvasClipboard, type SystemClipboard } from "./CanvasClipboard";

const generated = new URL("../../../generated/", import.meta.url);
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
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
  written: ClipboardItem[] = [];
  async write(items: ClipboardItem[]) {
    this.written = items;
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

describe("copying an image", () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 1]);
  const jpeg = new Uint8Array([0xff, 0xd8, 1]);
  const converted = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 9]);
  const codec = {
    size: async () => undefined,
    encode: async () => ({ bytes: converted, width: 1, height: 1 }),
  };
  class FakeItem {
    constructor(readonly data: Record<string, Promise<Blob>>) {}
  }
  vi.stubGlobal("ClipboardItem", FakeItem);
  afterEach(() => vi.clearAllMocks());

  async function copied(bytes: Uint8Array) {
    const editor = empty();
    editor.insertImages([{ fileId: "f1", mimeType: "image/png", bytes, width: 10, height: 10 }]);
    const system = new FakeClipboard();
    const clipboard = new CanvasClipboard(
      editor,
      system,
      { images: () => {}, changed: () => {} },
      codec,
    );
    await clipboard.copyImage();
    const [item] = system.written as unknown as FakeItem[];
    return item && new Uint8Array(await (await item.data["image/png"]).arrayBuffer());
  }

  it("writes a PNG as it is", async () => {
    expect(await copied(png)).toEqual(png);
  });

  it("converts other pictures to PNG", async () => {
    expect(await copied(jpeg)).toEqual(converted);
  });

  it("writes nothing with no single image selected", async () => {
    const system = new FakeClipboard();
    await setup(withSelection(), system).clipboard.copyImage();
    expect(system.written).toEqual([]);
  });
});

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

/** The `clipboardData` of a copy, cut or paste event, as a fake. */
class FakeTransfer {
  constructor(
    readonly data = new Map<string, string>(),
    readonly files: File[] = [],
  ) {}
  setData(type: string, text: string) {
    this.data.set(type, text);
  }
  getData(type: string) {
    return this.data.get(type) ?? "";
  }
}

/** Dispatches a clipboard event on `target`; whether the clipboard took it from the browser. */
function dispatch(target: EventTarget, type: string, transfer: FakeTransfer): boolean {
  const event = Object.assign(new Event(type, { cancelable: true }), { clipboardData: transfer });
  target.dispatchEvent(event);
  return event.defaultPrevented;
}

describe("the browser's copy, cut and paste (⌘/Ctrl+C, X, V and the Edit menu)", () => {
  const listening = (editor: Editor, point?: { x: number; y: number }) => {
    const page = new EventTarget();
    const parts = setup(editor);
    parts.clipboard.listen(page, () => point);
    return { page, ...parts };
  };

  it("copies the Selection into the event, which pastes into another Canvas at the pointer", () => {
    const transfer = new FakeTransfer();
    expect(dispatch(listening(withSelection()).page, "copy", transfer)).toBe(true);
    expect(transfer.getData("text/plain")).toContain(`"type":"excalidraw/clipboard"`);

    const target = empty();
    const pasting = listening(target, { x: 100, y: 80 });
    expect(dispatch(pasting.page, "paste", transfer)).toBe(true);
    expect(target.elementCount()).toBe(1);
    expect(new TextDecoder().decode(target.save())).toMatch(/"x": 50\b/);
    expect(pasting.changes()).toBe(1);
  });

  it("cuts the Selection into the event", () => {
    const editor = withSelection();
    const { page, changes } = listening(editor);
    const transfer = new FakeTransfer();
    expect(dispatch(page, "cut", transfer)).toBe(true);
    expect(transfer.getData("text/plain")).toContain("excalidraw/clipboard");
    expect(editor.elementCount()).toBe(0);
    expect(changes()).toBe(1);
  });

  it("leaves the browser alone with nothing selected and nothing of ours to paste", () => {
    const editor = empty();
    const { page, inserted } = listening(editor);
    expect(dispatch(page, "copy", new FakeTransfer())).toBe(false);
    expect(dispatch(page, "cut", new FakeTransfer())).toBe(false);
    const words = new FakeTransfer(new Map([["text/plain", "just some words"]]));
    expect(dispatch(page, "paste", words)).toBe(false);
    expect(editor.elementCount()).toBe(0);
    expect(inserted).toEqual([]);
  });

  it("pastes images at the pointer", async () => {
    const { page, inserted } = listening(empty(), { x: 5, y: 6 });
    const png = new File([new Uint8Array([1, 2, 3])], "a.png", { type: "image/png" });
    const text = new File(["x"], "a.txt", { type: "text/plain" });
    expect(dispatch(page, "paste", new FakeTransfer(new Map(), [png, text]))).toBe(true);
    await expect.poll(() => inserted.length).toBe(1);
    expect(inserted[0].sources.map((s) => [...s])).toEqual([[1, 2, 3]]);
    expect(inserted[0].at).toEqual({ x: 5, y: 6 });
  });

  it("leaves text fields their own copy, cut and paste", () => {
    const field = Object.assign(new EventTarget(), { tagName: "TEXTAREA" });
    const editor = withSelection();
    setup(editor).clipboard.listen(field, () => undefined);
    expect(dispatch(field, "copy", new FakeTransfer())).toBe(false);
    expect(dispatch(field, "cut", new FakeTransfer())).toBe(false);
    expect(editor.elementCount()).toBe(1);
  });

  it("stops listening when told", () => {
    const page = new EventTarget();
    const stop = setup(withSelection()).clipboard.listen(page, () => undefined);
    stop();
    expect(dispatch(page, "copy", new FakeTransfer())).toBe(false);
  });
});
