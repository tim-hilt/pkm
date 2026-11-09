import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, Tool, loadCore } from "../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { ActionRowModel, actionRowItems, type ActionRowItem } from "./ActionRowModel";

/** The phone Action row's items and popover, with sections from a real editor. */
let core: Core;
beforeAll(async () => {
  const generated = new URL("../../generated/", import.meta.url);
  const font = new Excalifont(
    new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
  );
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

const drawing =
  `{"type":"excalidraw","version":2,"elements":[` +
  `{"id":"box","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":2,` +
  `"strokeColor":"#1e1e1e","backgroundColor":"transparent","roundness":null,"version":3,"versionNonce":7},` +
  `{"id":"label","type":"text","x":300,"y":100,"width":60,"height":25,"text":"hi","fontSize":20,` +
  `"fontFamily":5,"textAlign":"left","verticalAlign":"top","originalText":"hi","autoResize":true,` +
  `"lineHeight":1.25,"strokeColor":"#1e1e1e","backgroundColor":"transparent","version":3,"versionNonce":9}]}`;

const makeEditor = () => {
  const editor = core.makeEditor();
  editor.load(new TextEncoder().encode(drawing));
  editor.setViewSize(600, 400);
  return editor;
};
const selectAll = (editor: Editor) => {
  editor.setTool(Tool.Selection);
  drag(editor, [
    [20, 20],
    [500, 300],
  ]);
};
const inputs = (editor: Editor) => ({
  sections: editor.showsProperties() ? editor.properties().sections : undefined,
  hasSelection: editor.selection().length > 0,
});
const items = (editor: Editor, width = 365) => actionRowItems({ ...inputs(editor), width });
const row = (editor: Editor, width = 365) => {
  const model = new ActionRowModel();
  model.update({ ...inputs(editor), width });
  return model;
};

describe("the Action row's items", () => {
  it("offers a rectangle Tool every style trigger but no text", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Rectangle);
    expect(items(editor)).toEqual(["stroke", "background", "properties"]);
  });

  it("has no background for an arrow, and a line is the same", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Arrow);
    const arrow = items(editor);
    expect(arrow).not.toContain("background");
    expect(arrow).toContain("stroke");
    expect(arrow).toContain("properties");
    editor.setTool(Tool.Line);
    expect(items(editor)).toContain("properties");
  });

  it("has no text trigger for freedraw", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Freedraw);
    expect(items(editor)).not.toContain("text");
    expect(items(editor)).toContain("stroke");
  });

  it("offers text stroke, text and properties for its opacity", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Text);
    expect(items(editor)).toEqual(["stroke", "properties", "text"]);
  });

  it("has every trigger that applies to any Element of a mixed Selection, and Actions", () => {
    const editor = makeEditor();
    selectAll(editor);
    expect(items(editor)).toEqual(["stroke", "background", "properties", "text", "actions"]);
  });

  it("is empty for hand, selection and eraser with nothing selected", () => {
    const editor = makeEditor();
    for (const tool of [Tool.Hand, Tool.Selection, Tool.Eraser]) {
      editor.setTool(tool);
      expect(items(editor), `${tool}`).toEqual([]);
    }
  });

  it("moves Duplicate and Delete onto the row by its width", () => {
    const editor = makeEditor();
    selectAll(editor);
    expect(items(editor, 365).slice(-1)).toEqual(["actions"]);
    expect(items(editor, 374).slice(-2)).toEqual(["actions", "delete"]);
    expect(items(editor, 412).slice(-3)).toEqual(["actions", "duplicate", "delete"]);
    expect(items(editor, 450).slice(-3)).toEqual(["actions", "duplicate", "delete"]);
  });
});

describe("the Action row's popover", () => {
  it("drops from the Actions popover what is on the row", () => {
    const editor = makeEditor();
    selectAll(editor);
    for (const [width, duplicate, del] of [
      [365, true, true],
      [374, true, false],
      [412, false, false],
      [450, false, false],
    ] as const) {
      const model = row(editor, width);
      expect([model.popoverHasDuplicate, model.popoverHasDelete], `${width}`).toEqual([
        duplicate,
        del,
      ]);
    }
  });

  it("opens, switches and closes by tapping triggers", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Rectangle);
    const model = row(editor);
    model.tap("stroke");
    expect(model.open).toBe("stroke");
    model.tap("properties");
    expect(model.open).toBe("properties");
    model.tap("properties");
    expect(model.open).toBeUndefined();
  });

  it("does not open for a trigger that acts at once or is not on the row", () => {
    const editor = makeEditor();
    selectAll(editor);
    const model = row(editor, 450);
    for (const item of ["delete", "duplicate", "stroke"] as ActionRowItem[]) {
      model.tap(item);
      expect(model.open).toBe(item === "stroke" ? "stroke" : undefined);
      model.close();
    }
    editor.setTool(Tool.Hand);
    const bare = row(editor);
    bare.tap("stroke");
    expect(bare.open).toBeUndefined();
  });

  it("closes on a Canvas touch, which is swallowed only when it was open", () => {
    const editor = makeEditor();
    editor.setTool(Tool.Rectangle);
    const model = row(editor);
    expect(model.canvasTouch()).toBe(false);
    model.tap("stroke");
    expect(model.canvasTouch()).toBe(true);
    expect(model.open).toBeUndefined();
  });

  it("closes when its trigger goes and stays while it remains", () => {
    const editor = makeEditor();
    selectAll(editor);
    const model = row(editor);
    model.tap("actions");
    model.update({ ...inputs(editor), width: 365 });
    expect(model.open).toBe("actions");

    editor.deleteSelection();
    model.update({ ...inputs(editor), width: 365 });
    expect(model.open).toBeUndefined();
  });
});
