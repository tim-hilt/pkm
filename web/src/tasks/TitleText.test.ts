import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { Inline, loadCore } from "../core/Core";
import { formatKey, titleSpans } from "./TitleText";

beforeAll(async () => {
  const generated = new URL("../../generated/", import.meta.url);
  await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

describe("titleSpans", () => {
  it("copies the core's spans with their marker lengths", () => {
    expect(titleSpans("*a* **b** `c`")).toEqual([
      { kind: Inline.Emphasis, start: 0, end: 3, markerLen: 1 },
      { kind: Inline.Strong, start: 4, end: 9, markerLen: 2 },
      { kind: Inline.Code, start: 10, end: 13, markerLen: 1 },
    ]);
  });
});

describe("formatKey", () => {
  const key = (key: string, modifiers: Partial<KeyboardEventInit> = {}) =>
    ({
      key,
      metaKey: false,
      ctrlKey: false,
      altKey: false,
      shiftKey: false,
      ...modifiers,
    }) as KeyboardEvent;

  it("takes ⌘ on Apple platforms and leaves Ctrl-B and Ctrl-E to the text field", () => {
    expect(formatKey(key("b", { metaKey: true }), true)).toBe(Inline.Strong);
    expect(formatKey(key("I", { metaKey: true }), true)).toBe(Inline.Emphasis);
    expect(formatKey(key("e", { ctrlKey: true }), true)).toBeUndefined();
  });

  it("takes Ctrl elsewhere", () => {
    expect(formatKey(key("e", { ctrlKey: true }), false)).toBe(Inline.Code);
    expect(formatKey(key("b", { metaKey: true }), false)).toBeUndefined();
  });

  it("ignores other keys and extra modifiers", () => {
    expect(formatKey(key("x", { metaKey: true }), true)).toBeUndefined();
    expect(formatKey(key("b", { metaKey: true, shiftKey: true }), true)).toBeUndefined();
    expect(formatKey(key("b"), true)).toBeUndefined();
  });
});
