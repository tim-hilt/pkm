import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, loadCore } from "../../core/Core";
import { tap } from "../testing/Gestures";
import { askUnit, parseUnit } from "./UnitPrompt";

const generated = new URL("../../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const withRectangle = () => {
  const editor = core.makeEditor();
  editor.load(
    new TextEncoder().encode(
      `{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":10,"y":20,"width":30,"height":40,"backgroundColor":"#ffc9c9"}]}`,
    ),
  );
  return editor;
};

describe("parsing a unit", () => {
  it("accepts numbers above zero", () => {
    expect(parseUnit("3")).toBe(3);
    expect(parseUnit(" 0.5 ")).toBe(0.5);
    expect(parseUnit("2,5")).toBe(2.5);
  });

  it("rejects zero, negative, non-numeric and empty text", () => {
    for (const text of ["0", "-1", "abc", "", "  ", "Infinity", "1e999"]) {
      expect(parseUnit(text)).toBeUndefined();
    }
  });
});

describe("Set unit…", () => {
  it("fills in the measure with two decimals and sets the Unit from the answer", () => {
    const editor = withRectangle();
    tap(editor, 20, 30);
    const asked: string[] = [];
    expect(
      askUnit(editor, (_message, initial) => {
        asked.push(initial);
        return "3";
      }),
    ).toBe(true);
    expect(asked).toEqual(["30.00"]);
    expect(editor.unit()).toBe(10);
    editor.free();
  });

  it("asks again for an invalid answer, and changes nothing when cancelled", () => {
    const editor = withRectangle();
    tap(editor, 20, 30);
    const answers = ["0", "abc", null];
    const asked: string[] = [];
    expect(
      askUnit(editor, (_message, initial) => {
        asked.push(initial);
        return answers.shift() ?? null;
      }),
    ).toBe(false);
    expect(asked).toEqual(["30.00", "0", "abc"]);
    expect(editor.unit()).toBe(1);
    editor.free();
  });

  it("does not ask when nothing can be measured", () => {
    const editor = withRectangle();
    expect(askUnit(editor, () => "3")).toBe(false);
    editor.free();
  });
});
