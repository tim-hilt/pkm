import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it, vi } from "vitest";
import { type Core, ExportScope, Tool, loadCore } from "../core/Core";
import { DrawingStore } from "../gallery/DrawingStore";
import { OpenDrawing } from "../gallery/OpenDrawing";
import { MemoryStorage } from "../storage/MemoryStorage";
import { drag } from "../testing/Gestures";
import { type DownloadedFile, DrawingExport } from "./DrawingExport";

const generated = new URL("../../generated/", import.meta.url);
const sample = new Uint8Array(readFileSync(new URL("sample.excalidraw", generated)));
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)));
});

async function open() {
  const store = new DrawingStore(new MemoryStorage());
  const drawing = await OpenDrawing.open(await store.add(sample, "Sample plan"), store, core);
  const saved: DownloadedFile[] = [];
  const png = vi.fn(async () => new Uint8Array([1, 2, 3]) as Uint8Array | undefined);
  const exporter = new DrawingExport(drawing, { save: (file) => void saved.push(file), png });
  return { drawing, store, saved, png, exporter };
}

describe("downloading the open Drawing", () => {
  it("is the stored .excalidraw file byte for byte, named after the Drawing", async () => {
    const { exporter, saved, store } = await open();
    await exporter.downloadDrawing();
    expect(saved).toHaveLength(1);
    expect(saved[0].name).toBe("Sample plan.excalidraw");
    expect(saved[0].bytes).toEqual(await store.open("Sample plan"));
    expect(saved[0].bytes).toEqual(sample);
  });

  it("holds changes not yet autosaved", async () => {
    const { exporter, saved, drawing } = await open();
    drawing.editor.setTool(Tool.Rectangle);
    drag(drawing.editor, [
      [10, 10],
      [90, 60],
    ]);
    drawing.editorDidChange();

    await exporter.downloadDrawing();

    const editor = core.makeEditor();
    editor.load(saved[0].bytes);
    expect(editor.elementCount()).toBe(drawing.editor.elementCount());
    editor.free();
  });

  it("downloads a PNG named after the Drawing, as asked for", async () => {
    const { exporter, saved, png } = await open();
    await exporter.downloadPng(ExportScope.Selection, false);
    expect(png).toHaveBeenCalledWith(ExportScope.Selection, false);
    expect(saved).toEqual([
      { name: "Sample plan.png", bytes: new Uint8Array([1, 2, 3]), mimeType: "image/png" },
    ]);
  });

  it("downloads no PNG when there is nothing to export", async () => {
    const { exporter, saved, png } = await open();
    png.mockResolvedValue(undefined);
    await exporter.downloadPng(ExportScope.Selection, true);
    expect(saved).toEqual([]);
  });
});
