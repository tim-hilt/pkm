import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, Tool, loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import { drag } from "../testing/Gestures";
import { isApplePlatform, isCopyPngShortcut, shortcutOf, type KeyLike } from "./KeyboardShortcuts";

const generated = new URL("../../../generated/", import.meta.url);
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

const key = (key: string, extra: Partial<KeyLike> = {}): KeyLike => ({
  key,
  code: "",
  metaKey: false,
  ctrlKey: false,
  altKey: false,
  shiftKey: false,
  ...extra,
});
const none = { shift: false, ctrl: false, alt: false, meta: false };

describe("the primary modifier", () => {
  it("is ⌘ on Apple platforms", () => {
    expect(shortcutOf(key("z", { metaKey: true }), true)).toEqual({
      key: "z",
      modifiers: { ...none, meta: true },
    });
  });

  it("is Ctrl elsewhere, where the core's ⌘ shortcuts are", () => {
    expect(shortcutOf(key("z", { ctrlKey: true }), false)).toEqual({
      key: "z",
      modifiers: { ...none, meta: true },
    });
  });

  it("is not the Windows key, and Ctrl is only Ctrl on Apple platforms", () => {
    expect(shortcutOf(key("z", { metaKey: true }), false)?.modifiers.meta).toBe(false);
    expect(shortcutOf(key("z", { ctrlKey: true }), true)?.modifiers).toEqual({
      ...none,
      ctrl: true,
    });
  });

  it("is told from the platform's name", () => {
    expect(isApplePlatform("MacIntel")).toBe(true);
    expect(isApplePlatform("iPad")).toBe(true);
    expect(isApplePlatform("Win32")).toBe(false);
    expect(isApplePlatform("Linux x86_64")).toBe(false);
  });
});

describe("the key's name", () => {
  it("is the lower-case character typed", () => {
    expect(shortcutOf(key("R"), true)?.key).toBe("r");
  });

  it("names the keys that type nothing", () => {
    expect(shortcutOf(key("Backspace"), true)?.key).toBe("backspace");
    expect(shortcutOf(key("Delete"), true)?.key).toBe("delete");
    expect(shortcutOf(key("Escape"), true)?.key).toBe("escape");
    expect(shortcutOf(key("Enter"), true)?.key).toBe("enter");
    expect(shortcutOf(key("ArrowUp", { shiftKey: true }), true)?.key).toBe("arrowup");
  });

  it("is the character without Shift, as the core has it", () => {
    expect(shortcutOf(key("!", { code: "Digit1", shiftKey: true }), true)).toEqual({
      key: "1",
      modifiers: { ...none, shift: true },
    });
  });

  it("is the letter typed with Shift, wherever the layout has it", () => {
    // ⌘⇧Z on a German keyboard: Z is where US keyboards have Y.
    expect(shortcutOf(key("Z", { code: "KeyY", metaKey: true, shiftKey: true }), true)).toEqual({
      key: "z",
      modifiers: { ...none, shift: true, meta: true },
    });
  });

  it("is the character without Alt, which types another on a Mac", () => {
    expect(
      shortcutOf(key("‘", { code: "BracketRight", metaKey: true, altKey: true }), true)?.key,
    ).toBe("]");
  });

  it("is nothing for a modifier or a function key on its own", () => {
    expect(shortcutOf(key("Shift"), true)).toBeUndefined();
    expect(shortcutOf(key("F5"), true)).toBeUndefined();
    expect(shortcutOf(key("Dead"), true)).toBeUndefined();
  });
});

describe("pressing a key on the core", () => {
  const press = (editor: Editor, k: KeyLike, apple = true) => {
    const shortcut = shortcutOf(k, apple);
    return shortcut ? editor.pressKey(shortcut.key, shortcut.modifiers) : false;
  };

  it("selects the Tool of a letter", () => {
    const editor = core.makeEditor();
    expect(press(editor, key("r"))).toBe(true);
    expect(editor.tool()).toBe(Tool.Rectangle);
    editor.free();
  });

  it("leaves a key without a Command alone", () => {
    const editor = core.makeEditor();
    expect(press(editor, key("b"))).toBe(false);
    editor.free();
  });

  it("undoes with Ctrl+Z outside of Apple platforms", () => {
    const editor = core.makeEditor();
    editor.setViewSize(400, 300);
    editor.setTool(Tool.Rectangle);
    drag(editor, [
      [100, 100],
      [200, 160],
    ]);
    expect(editor.elementCount()).toBe(1);
    expect(press(editor, key("z", { ctrlKey: true }), false)).toBe(true);
    expect(editor.elementCount()).toBe(0);
    editor.free();
  });

  it("zooms in with Ctrl+= and with ⌘+=", () => {
    for (const [apple, k] of [
      [false, key("=", { ctrlKey: true })],
      [true, key("=", { metaKey: true })],
    ] as const) {
      const editor = core.makeEditor();
      editor.setViewSize(400, 300);
      const before = editor.viewport().zoom;
      expect(press(editor, k, apple)).toBe(true);
      expect(editor.viewport().zoom).toBeGreaterThan(before);
      editor.free();
    }
  });
});

describe("Copy as PNG", () => {
  it("is ⇧⌥C, as on excalidraw.com, on every platform", () => {
    // On a Mac ⌥⇧C types "Ç".
    const mac = key("Ç", { code: "KeyC", altKey: true, shiftKey: true });
    const other = key("C", { code: "KeyC", altKey: true, shiftKey: true });
    expect(isCopyPngShortcut(shortcutOf(mac, true)!)).toBe(true);
    expect(isCopyPngShortcut(shortcutOf(other, false)!)).toBe(true);
  });

  it("is not C with other modifiers", () => {
    for (const extra of [
      {},
      { altKey: true },
      { shiftKey: true },
      { altKey: true, shiftKey: true, metaKey: true },
    ]) {
      expect(isCopyPngShortcut(shortcutOf(key("c", { code: "KeyC", ...extra }), true)!)).toBe(
        false,
      );
    }
  });
});
