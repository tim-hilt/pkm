import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { dashOf, fillKind, stringOf, strokeKind, verbsOf } from "../rendering/GeometryBuffers";
import { Excalifont } from "../text/Excalifont";
import { type Core, loadCore } from "./Core";

const generated = new URL("../../generated/", import.meta.url);
const sample = readFileSync(new URL("sample.excalidraw", generated));

const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);

let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const drawing = (elements: string) =>
  new TextEncoder().encode(`{"type":"excalidraw","version":2,"elements":[${elements}]}`);

describe("the core as wasm", () => {
  it("round-trips a Scene and keeps what it does not know", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":10,"y":20,"width":100,"height":50,"futureProperty":{"a":[1,2]}}`,
      ),
    );
    expect(editor.elementCount()).toBe(1);

    const saved = JSON.parse(new TextDecoder().decode(editor.save()));
    expect(saved.elements[0].futureProperty).toEqual({ a: [1, 2] });
    editor.free();
  });

  it("rejects a file that is not a Drawing", () => {
    const editor = core.makeEditor();
    expect(() => editor.load(new TextEncoder().encode("not json"))).toThrow();
    editor.free();
  });

  it("reads the geometry of a rectangle through views onto wasm memory", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":50,"y":100,"width":280,"height":180,"strokeColor":"#1e1e1e","strokeWidth":2,"backgroundColor":"#a5d8ff","fillStyle":"solid"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      expect(buffers.parts.length).toBe(2);
      const fill = buffers.parts.at(0);
      expect(fill.kind).toBe(fillKind);
      expect(fill.color).toBe(0xa5d8ffff);
      const stroke = buffers.parts.at(1);
      expect(stroke.kind).toBe(strokeKind);
      expect(stroke.color).toBe(0x1e1e1eff);
      expect(stroke.width).toBe(2);
      expect(verbsOf(buffers, stroke)[0]).toBe(0); // starts with a move
      expect(stroke.verbCount).toBeGreaterThan(0);
      expect(buffers.coords.length).toBeGreaterThan(0);
    });
    geometry.free();
    editor.free();
  });

  it("carries a dashed stroke's dash pattern", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"r","type":"rectangle","x":0,"y":0,"width":100,"height":50,"strokeStyle":"dashed"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      expect(dashOf(buffers.parts.at(0))).toEqual({ on: 8, off: 10 });
    });
    geometry.free();
    editor.free();
  });

  it("reads geometry correctly after memory grew", () => {
    const editor = core.makeEditor();
    editor.load(sample);
    const geometry = editor.geometry();
    const read = () => geometry.withBuffers((buffers) => buffers.parts.at(0).color);
    const before = read();
    // Growing memory detaches the buffer the views were made on.
    core.memory.grow(16);
    expect(read()).toBe(before);
    geometry.free();
    editor.free();
  });

  it("reports the Canvas colour and the viewport of the Drawing", () => {
    const editor = core.makeEditor();
    editor.load(sample);
    editor.setViewSize(800, 600);
    expect(editor.canvasColor()).toBe(0xffffffff);
    expect(editor.viewport().zoom).toBeGreaterThan(0);
    editor.setDarkMode(true);
    expect(editor.canvasColor()).not.toBe(0xffffffff);
    editor.free();
  });

  it("wraps a Label to its container with the font's measurements", () => {
    const editor = core.makeEditor();
    editor.load(
      drawing(
        `{"id":"box","type":"rectangle","x":0,"y":0,"width":120,"height":200,"roughness":0,"boundElements":[{"type":"text","id":"t"}]},` +
          `{"id":"t","type":"text","x":0,"y":0,"width":10,"height":25,"text":"A label that wraps","originalText":"A label that wraps","fontSize":20,"fontFamily":5,"lineHeight":1.25,"containerId":"box","textAlign":"center","verticalAlign":"middle"}`,
      ),
    );
    const geometry = editor.geometry();
    geometry.withBuffers((buffers) => {
      const lines = Array.from({ length: buffers.textLines.length }, (_, i) =>
        stringOf(buffers, buffers.textLines.at(i)),
      );
      expect(lines.join(" ")).toBe("A label that wraps");
      expect(lines.length).toBeGreaterThan(1);
      // The container is 120 wide with 5 of padding on each side.
      for (const line of lines) expect(font.lineWidth(line, 20)).toBeLessThanOrEqual(110);
    });
    geometry.free();
    editor.free();
  });
});
