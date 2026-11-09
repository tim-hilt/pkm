import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, Tool, loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { optionGroups } from "./PropertyGroups";

let core: Core;
beforeAll(async () => {
  const generated = new URL("../../../generated/", import.meta.url);
  const font = new Excalifont(
    new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
  );
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

describe("the option groups of the Properties panel", () => {
  it("show what the editor holds after each option is picked", () => {
    const editor = core.makeEditor();
    editor.setTool(Tool.Text);
    for (const group of optionGroups) {
      for (const option of group.options) {
        editor.setProperty(group.property(option.value));
        expect(group.chosen(editor.properties()), `${group.title}: ${option.name}`).toBe(
          option.value,
        );
      }
    }
    editor.free();
  });

  it("have names for every option, for screen readers", () => {
    for (const group of optionGroups) {
      for (const option of group.options) expect(option.name).not.toBe("");
    }
  });

  it("apply as the core's sections say", () => {
    const editor = core.makeEditor();
    editor.setTool(Tool.Arrow);
    const arrow = editor.properties();
    expect(optionGroups.filter((g) => g.applies(arrow)).map((g) => g.title)).toEqual([
      "Stroke width",
      "Stroke style",
      "Sloppiness",
    ]);
    editor.setTool(Tool.Text);
    const text = editor.properties();
    expect(optionGroups.some((g) => g.title === "Font size" && g.applies(text))).toBe(true);
    expect(optionGroups.some((g) => g.title === "Edges" && g.applies(text))).toBe(false);
    editor.free();
  });

  it("show none chosen where the Selection's Elements differ", () => {
    const editor = core.makeEditor();
    editor.load(
      new TextEncoder().encode(
        `{"type":"excalidraw","version":2,"elements":[` +
          `{"id":"a","type":"rectangle","x":100,"y":100,"width":100,"height":50,"strokeWidth":1},` +
          `{"id":"b","type":"rectangle","x":300,"y":100,"width":100,"height":50,"strokeWidth":4}]}`,
      ),
    );
    editor.setViewSize(600, 400);
    editor.setTool(Tool.Selection);
    drag(editor, [[100, 125]]);
    drag(editor, [[300, 125]].map(([x, y]) => [x, y]) as [number, number][], undefined, undefined, {
      shift: true,
      ctrl: false,
      alt: false,
      meta: false,
    });
    expect(editor.selection()).toHaveLength(2);
    expect(editor.properties().strokeWidth).toBeUndefined();
    editor.free();
  });
});
