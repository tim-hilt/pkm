import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, PointerKind, loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import { CanvasClipboard, type SystemClipboard } from "../input/CanvasClipboard";
import { type MenuEntry, openContextMenu } from "./ContextMenuModel";

const generated = new URL("../../../generated/", import.meta.url);
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

const drawing = new TextEncoder().encode(
  `{"type":"excalidraw","version":2,"elements":[
  {"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7},
  {"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,"strokeWidth":2,"backgroundColor":"#ffc9c9","roundness":null,"version":3,"versionNonce":7}]}`,
);

function makeEditor(): Editor {
  const editor = core.makeEditor();
  editor.setViewSize(800, 600);
  editor.load(drawing);
  return editor;
}

/** A clipboard that remembers what it was asked. */
function fakeClipboard() {
  const calls: string[] = [];
  const at: ({ x: number; y: number } | undefined)[] = [];
  return {
    calls,
    at,
    copy: async () => void calls.push("copy"),
    copyImage: async () => void calls.push("copyImage"),
    cut: async () => void calls.push("cut"),
    paste: async (point?: { x: number; y: number }) => {
      calls.push("paste");
      at.push(point);
    },
  };
}

const labels = (entries: MenuEntry[]) => entries.map((e) => e.label);
const entry = (entries: MenuEntry[], label: string): MenuEntry =>
  entries.find((e) => e.label === label)!;
const onElement = { x: 150, y: 125 };
const onCanvas = { x: 600, y: 500 };

function open(editor: Editor, at = onElement, kind = PointerKind.Finger) {
  const clipboard = fakeClipboard();
  const entries = openContextMenu(editor, clipboard, at, kind);
  return { entries, clipboard };
}

describe("the Element menu", () => {
  it("has the entries in order, with Arrange holding the z-order moves", () => {
    const { entries } = open(makeEditor());

    expect(labels(entries!)).toEqual([
      "Cut",
      "Copy",
      "Paste",
      "Duplicate",
      "Group",
      "Ungroup",
      "Arrange",
      "Delete",
    ]);
    expect(labels(entry(entries!, "Arrange").children!)).toEqual([
      "Bring to Front",
      "Bring Forward",
      "Send Backward",
      "Send to Back",
    ]);
    expect(entry(entries!, "Delete").destructive).toBe(true);
  });

  it("opens on an Element, selecting it", () => {
    const editor = makeEditor();
    open(editor);
    expect(editor.selection()).toEqual(["a"]);
  });

  it("enables Group and Ungroup as the core says", () => {
    const editor = makeEditor();
    editor.selectAll();
    let { entries } = open(editor);
    expect(entry(entries!, "Group").disabled).toBeFalsy();
    expect(entry(entries!, "Ungroup").disabled).toBe(true);

    entry(entries!, "Group").run!();
    ({ entries } = open(editor));
    expect(entry(entries!, "Group").disabled).toBe(true);
    expect(entry(entries!, "Ungroup").disabled).toBeFalsy();
  });

  it("does with each entry what the matching command does", () => {
    const editor = makeEditor();
    editor.selectAll();
    const { entries } = open(editor);

    entry(entries!, "Duplicate").run!();
    expect(editor.elementCount()).toBe(4);
    entry(entries!, "Delete").run!();
    expect(editor.elementCount()).toBe(2);
  });

  it("moves the Selection in z-order from Arrange", () => {
    const editor = makeEditor();
    const { entries } = open(editor);

    entry(entry(entries!, "Arrange").children!, "Bring to Front").run!();

    const saved = new TextDecoder().decode(editor.save());
    expect(saved.indexOf('"id": "b"')).toBeLessThan(saved.indexOf('"id": "a"'));
  });

  it("copies, cuts and pastes through the clipboard, pasting where it was opened", () => {
    const { entries, clipboard } = open(makeEditor());

    entry(entries!, "Copy").run!();
    entry(entries!, "Cut").run!();
    entry(entries!, "Paste").run!();

    expect(clipboard.calls).toEqual(["copy", "cut", "paste"]);
    expect(clipboard.at).toEqual([onElement]);
  });
});

describe("the Element menu of an image", () => {
  const image = {
    fileId: "f1",
    mimeType: "image/png",
    bytes: new Uint8Array([1, 2, 3]),
    width: 100,
    height: 50,
  };

  it("has Copy Image after Copy, running the clipboard's copyImage", () => {
    const editor = makeEditor();
    editor.insertImages([image]);
    const { entries, clipboard } = open(editor, { x: 400, y: 300 });

    expect(labels(entries!).slice(0, 4)).toEqual(["Cut", "Copy", "Copy Image", "Paste"]);
    entry(entries!, "Copy Image").run!();
    expect(clipboard.calls).toEqual(["copyImage"]);
  });

  it("is left out for other Selections", () => {
    const editor = makeEditor();
    editor.insertImages([image]);
    editor.selectAll();

    expect(labels(open(editor, onElement).entries!)).not.toContain("Copy Image");
    expect(labels(open(makeEditor()).entries!)).not.toContain("Copy Image");
  });
});

describe("the Canvas menu", () => {
  it("has Paste and Select All and keeps the Selection", () => {
    const editor = makeEditor();
    open(editor);

    const { entries } = open(editor, onCanvas);

    expect(labels(entries!)).toEqual(["Paste", "Select All"]);
    expect(editor.selection()).toEqual(["a"]);
    entry(entries!, "Select All").run!();
    expect(new Set(editor.selection())).toEqual(new Set(["a", "b"]));
  });

  it("pastes around the point it was opened at", async () => {
    const editor = makeEditor();
    editor.selectAll();
    const text = editor.copySelection()!;
    editor.deleteSelection();
    const system: SystemClipboard = {
      writeText: async () => {},
      write: async () => {},
      read: async () => [
        { types: ["text/plain"], getType: async () => new Blob([text], { type: "text/plain" }) },
      ],
    };
    const clipboard = new CanvasClipboard(editor, system, {
      images() {},
      changed() {},
    });

    const entries = openContextMenu(editor, clipboard, onCanvas, PointerKind.Mouse)!;
    await entry(entries, "Paste").run!();

    expect(editor.elementCount()).toBe(2);
    // The two rectangles span 300 wide, centred on the press point.
    expect(new TextDecoder().decode(editor.save())).toMatch(/"x": 450(\.|,)/);
  });
});

describe("a press that means nothing", () => {
  it("opens no menu for the Pencil and selects nothing", () => {
    const editor = makeEditor();

    expect(open(editor, onElement, PointerKind.Pencil).entries).toBeUndefined();
    expect(open(editor, onCanvas, PointerKind.Pencil).entries).toBeUndefined();
    expect(editor.selection()).toEqual([]);
  });
});
