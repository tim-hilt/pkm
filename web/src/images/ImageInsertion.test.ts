import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, loadCore } from "../core/Core";
import { Excalifont } from "../text/Excalifont";
import { Tool } from "../core/Core";
import { insertImages } from "./ImageInsertion";
import type { ImageCodec } from "./ImageImporter";

/**
 * Pasting and dropping images through the Canvas: the pictures are prepared
 * for the core, inserted as Elements and kept in the Drawing. A port of the
 * Swift `InsertImagesTests`; the platform's drag and paste events only deliver
 * bytes to `insertImages`, which is where these tests start.
 */

const generated = new URL("../../generated/", import.meta.url);
const font = new Excalifont(
  new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)), font);
});

const png = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 7]);
const text = new TextEncoder().encode("hello");

/** Every file starting like a PNG is a 40 x 40 picture; other bytes are no image. */
const codec: ImageCodec = {
  async size(bytes) {
    return bytes[0] === 0x89 ? { width: 40, height: 40 } : undefined;
  },
  async encode() {
    throw new Error("small pictures are kept as they are");
  },
};

function makeEditor(): Editor {
  const editor = core.makeEditor();
  editor.setViewSize(800, 600);
  return editor;
}

describe("insertImages", () => {
  it("makes a pasted image the selected image Element", async () => {
    const editor = makeEditor();

    const inserted = await insertImages(editor, [png], undefined, codec);

    expect(inserted).toBe(true);
    expect(editor.elementCount()).toBe(1);
    expect(editor.selection()).toHaveLength(1);
    expect(editor.canUndo()).toBe(true);
    editor.free();
  });

  it("centres a dropped image on the drop point", async () => {
    const editor = makeEditor();

    await insertImages(editor, [png], { x: 100, y: 80 }, codec);

    const saved = new TextDecoder().decode(editor.save());
    expect(saved).toMatch(/"x": 80\b/);
    expect(saved).toMatch(/"y": 60\b/);
    editor.free();
  });

  it("stores the file's bytes in the Drawing under its id", async () => {
    const editor = makeEditor();

    await insertImages(editor, [png], undefined, codec);

    const id = /"fileId": "([^"]+)"/.exec(new TextDecoder().decode(editor.save()))![1];
    expect(editor.fileBytes(id)).toEqual(png);
    editor.free();
  });

  it("is undoable", async () => {
    const editor = makeEditor();
    await insertImages(editor, [png], undefined, codec);

    editor.undo();

    expect(editor.elementCount()).toBe(0);
    editor.free();
  });

  it("skips what is no image and does nothing if nothing is", async () => {
    const editor = makeEditor();

    expect(await insertImages(editor, [text], undefined, codec)).toBe(false);

    expect(editor.elementCount()).toBe(0);
    expect(editor.canUndo()).toBe(false);
    editor.free();
  });

  it("inserts the images among the sources and skips the others", async () => {
    const editor = makeEditor();
    const other = Uint8Array.from([...png, 8]);

    await insertImages(editor, [png, text, other], undefined, codec);

    expect(editor.elementCount()).toBe(2);
    expect(editor.selection()).toHaveLength(2);
    editor.free();
  });

  it("returns the Tool to the Selection", async () => {
    const editor = makeEditor();
    editor.setTool(Tool.Rectangle);

    await insertImages(editor, [png], undefined, codec);

    expect(editor.tool()).toBe(Tool.Selection);
    editor.free();
  });
});

describe("images in the core's geometry", () => {
  it("are read in place with their file's id", async () => {
    const editor = makeEditor();
    await insertImages(editor, [png], undefined, codec);

    const geometry = editor.geometry();
    const seen = geometry.withBuffers((buffers) => ({
      count: buffers.images.length,
      width: buffers.images.at(0).width,
      id: new TextDecoder().decode(buffers.fileIds),
    }));
    geometry.free();

    expect(seen.count).toBe(1);
    expect(seen.width).toBeGreaterThan(0);
    expect(seen.id).toMatch(/^[0-9a-f]{40}$/);
    editor.free();
  });
});
