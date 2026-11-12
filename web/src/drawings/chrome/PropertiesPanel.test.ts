import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { Alignment, type Core, Distribution, StrokeWidth, Tool, loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";

/**
 * The Properties panel's commands through the editor: they reach the core and
 * the panel's state follows the editor. What each property does is tested in
 * the core.
 */
let core: Core;
beforeAll(async () => {
  const generated = new URL("../../../generated/", import.meta.url);
  const font = new Excalifont(
    new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
  );
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

const drawing =
  `{"type":"excalidraw","version":2,"elements":[` +
  `{"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,` +
  `"strokeColor":"#1e1e1e","backgroundColor":"transparent","roundness":null,"version":3,"versionNonce":7}]}`;

const makeEditor = () => {
  const editor = core.makeEditor();
  editor.load(new TextEncoder().encode(drawing));
  editor.setViewSize(600, 400);
  return editor;
};
const select = (editor: ReturnType<typeof makeEditor>) => drag(editor, [[100, 125]]);

describe("the Properties panel's commands", () => {
  it("are, with nothing selected, the style of the next Element", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Rectangle);
    expect(editor.properties().strokeColor).toBe("#1e1e1e");

    editor.setProperty({ strokeColor: "#e03131" });

    expect(editor.properties().strokeColor).toBe("#e03131");
    editor.free();
  });

  it("follow the sections the core says apply", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Text);
    const text = editor.properties().sections;
    expect(text.text && !text.edges && !text.strokeWidth).toBe(true);
    editor.setTool(Tool.Arrow);
    const arrow = editor.properties().sections;
    expect(!arrow.text && !arrow.edges && arrow.strokeWidth).toBe(true);
    editor.free();
  });

  it("restyle the Selection and can be undone", () => {
    const editor = makeEditor();
    select(editor);
    expect(editor.selection()).toEqual(["box"]);
    editor.setProperty({ strokeWidth: StrokeWidth.Bold });
    expect(editor.properties().strokeWidth).toBe(StrokeWidth.Bold);
    expect(editor.canUndo()).toBe(true);

    editor.undo();

    expect(editor.properties().strokeWidth).toBe(StrokeWidth.Medium);
    editor.free();
  });

  it("show the panel for a drawing Tool or a Selection", () => {
    const editor = makeEditor();
    expect(editor.showsProperties()).toBe(false);
    editor.setTool(Tool.Rectangle);
    expect(editor.showsProperties()).toBe(true);
    editor.setTool(Tool.Selection);
    expect(editor.showsProperties()).toBe(false);
    select(editor);
    expect(editor.showsProperties()).toBe(true);
    editor.free();
  });

  it("align and distribute two and three Elements", () => {
    const editor = makeEditor();
    select(editor);
    expect(editor.properties().sections.align).toBe(false);
    editor.duplicate();
    editor.duplicate();
    editor.selectAll();
    expect(editor.properties().sections).toMatchObject({ align: true, distribute: true });

    editor.align(Alignment.Left);
    editor.undo();
    expect(editor.canRedo()).toBe(true);
    // The copies are spaced evenly already.
    editor.distribute(Distribution.Vertically);
    expect(editor.canRedo()).toBe(true);
    editor.free();
  });

  it("duplicate, reorder and delete the Selection", () => {
    const editor = makeEditor();
    select(editor);
    editor.duplicate();
    expect(editor.elementCount()).toBe(2);
    editor.deleteSelection();
    expect(editor.elementCount()).toBe(1);
    editor.free();
  });
});
