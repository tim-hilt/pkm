import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, PointerKind, Tool, loadCore } from "../core/Core";
import { drag } from "../testing/Gestures";
import { DrawingSettings, type SettingsStorage } from "./DrawingSettings";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const memory = (): SettingsStorage & { values: Map<string, string> } => {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
  };
};

describe("the settings of the main menu", () => {
  it("puts a white background behind PNG exports until told not to, and remembers", () => {
    const storage = memory();
    expect(new DrawingSettings(storage).exportBackground).toBe(true);
    const settings = new DrawingSettings(storage);
    settings.exportBackground = false;
    expect(new DrawingSettings(storage).exportBackground).toBe(false);
    settings.exportBackground = true;
    expect(new DrawingSettings(storage).exportBackground).toBe(true);
  });

  it("leaves Pencil-only drawing to the core until chosen", () => {
    const editor = core.makeEditor();
    const settings = new DrawingSettings(memory());
    expect(settings.pencilOnlyDrawing(editor)).toBe(false);
    editor.free();
  });

  it("remembers the user's choice for the next Drawing, off as well as on", () => {
    const storage = memory();
    const first = core.makeEditor();
    new DrawingSettings(storage).setPencilOnlyDrawing(first, false);

    const second = core.makeEditor();
    const settings = new DrawingSettings(storage);
    expect(settings.pencilOnlyDrawing(second)).toBe(false);
    drag(
      second,
      [
        [1, 1],
        [5, 5],
      ],
      PointerKind.Pencil,
    );
    expect(settings.pencilOnlyDrawing(second)).toBe(false);
    first.free();
    second.free();
  });

  it("keeps what the core turned on by itself with a pen", () => {
    const storage = memory();
    const editor = core.makeEditor();
    const settings = new DrawingSettings(storage);
    settings.pencilOnlyDrawing(editor);
    editor.setTool(Tool.Rectangle);
    drag(
      editor,
      [
        [1, 1],
        [50, 50],
      ],
      PointerKind.Pencil,
    );
    expect(settings.pencilOnlyDrawing(editor)).toBe(true);

    const next = core.makeEditor();
    expect(new DrawingSettings(storage).pencilOnlyDrawing(next)).toBe(true);
    editor.free();
    next.free();
  });
});
