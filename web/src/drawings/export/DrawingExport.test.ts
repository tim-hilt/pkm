import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { type Core, ExportScope, Tool, loadCore } from "../../core/Core";
import { DrawingStore } from "../gallery/DrawingStore";
import { OpenDrawing } from "../gallery/OpenDrawing";
import { MemoryStorage } from "../../storage/MemoryStorage";
import { drag } from "../testing/Gestures";
import { type DownloadedFile, DrawingExport } from "./DrawingExport";

const generated = new URL("../../../generated/", import.meta.url);
const sample = new Uint8Array(readFileSync(new URL("sample.excalidraw", generated)));
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

async function open() {
  const store = new DrawingStore(new MemoryStorage(), core);
  const drawing = await OpenDrawing.open(await store.add(sample, "Sample plan"), store, core);
  const saved: DownloadedFile[] = [];
  const copied: Uint8Array[] = [];
  const png = vi.fn(
    async (scope: ExportScope) => new Uint8Array([scope]) as Uint8Array | undefined,
  );
  const copy = vi.fn(async (bytes: Uint8Array) => void copied.push(bytes));
  const exporter = new DrawingExport(drawing, { save: (file) => void saved.push(file), png, copy });
  return { drawing, store, saved, copied, png, copy, exporter };
}

describe("Save as", () => {
  it("is the stored .excalidraw file byte for byte, named after the Drawing", async () => {
    const { exporter, saved, store } = await open();
    await exporter.saveAs();
    expect(saved).toHaveLength(1);
    expect(saved[0].name).toBe("Sample plan.excalidraw");
    expect(saved[0].bytes).toEqual(await store.open("Sample plan"));
  });

  it("holds changes not yet autosaved", async () => {
    const { exporter, saved, drawing } = await open();
    drawing.editor.setTool(Tool.Rectangle);
    drag(drawing.editor, [
      [10, 10],
      [90, 60],
    ]);
    drawing.editorDidChange();

    await exporter.saveAs();

    const editor = core.makeEditor();
    editor.load(saved[0].bytes);
    expect(editor.elementCount()).toBe(drawing.editor.elementCount());
    editor.free();
  });
});

describe("Export", () => {
  it("is of the Scene with nothing selected", async () => {
    const { exporter, drawing } = await open();
    expect(drawing.editor.selection()).toEqual([]);
    expect(await exporter.image()).toEqual(new Uint8Array([ExportScope.Scene]));
  });

  it("is of the Selection", async () => {
    const { exporter, drawing } = await open();
    drawing.editor.selectAll();
    expect(await exporter.image()).toEqual(new Uint8Array([ExportScope.Selection]));
  });

  it("downloads the PNG named after the Drawing", async () => {
    const { exporter, saved } = await open();
    exporter.downloadPng(new Uint8Array([1, 2, 3]));
    expect(saved).toEqual([
      { name: "Sample plan.png", bytes: new Uint8Array([1, 2, 3]), mimeType: "image/png" },
    ]);
  });

  it("copies the PNG to the clipboard, and says whether it could", async () => {
    const { exporter, copied, copy } = await open();
    expect(await exporter.copyPng(new Uint8Array([1]))).toBe(true);
    expect(copied).toEqual([new Uint8Array([1])]);
    copy.mockRejectedValue(new DOMException("denied", "NotAllowedError"));
    expect(await exporter.copyPng(new Uint8Array([2]))).toBe(false);
  });
});
