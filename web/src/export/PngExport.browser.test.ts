import { beforeAll, describe, expect, it } from "vitest";
import { type Editor, ExportScope, Tool } from "../core/Core";
import { type RenderedImage, bytesPerPixel, pixelAt, redAt } from "../rendering/RenderedImage";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";
import { drag } from "../testing/Gestures";
import { exportPng, renderExport } from "./PngExport";

/**
 * PNG export: the Scene or the Selection, drawn at 2x by the WebGPU renderer
 * offscreen, with or without the paper behind it. A port of the Swift
 * `PNGExportTests`.
 */

let harness: RenderHarness;
beforeAll(async () => {
  harness = await makeRenderHarness();
});

const encoder = new TextEncoder();

/** Two 100 x 50 rectangles at (100, 100) and (300, 100). */
function twoRectangles(): Editor {
  const editor = harness.core.makeEditor();
  const rectangle = (id: string, x: number) =>
    `{"id":"${id}","type":"rectangle","x":${x},"y":100,"width":100,"height":50,"backgroundColor":"transparent","roundness":null}`;
  editor.load(
    encoder.encode(
      `{"type":"excalidraw","version":2,"elements":[${rectangle("a", 100)},${rectangle("b", 300)}]}`,
    ),
  );
  editor.setViewSize(800, 600);
  editor.setViewport({ originX: 0, originY: 0, zoom: 1 });
  editor.setTool(Tool.Selection);
  return editor;
}

const render = (editor: Editor, scope: ExportScope, background: boolean) =>
  renderExport(harness.device, harness.font, editor, scope, background);

const alpha = (image: RenderedImage, x: number, y: number) =>
  image.rgba[(y * image.width + x) * bytesPerPixel + 3];

function hasInk(image: RenderedImage): boolean {
  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width; x++) {
      if (alpha(image, x, y) > 0 && redAt(image, x, y) < 128) return true;
    }
  }
  return false;
}

describe("PNG export", () => {
  it("exports the Scene at twice its size with padding", async () => {
    const image = (await render(twoRectangles(), ExportScope.Scene, true))!;

    // (300 + 2 * 10) x (50 + 2 * 10) at 2x
    expect([image.width, image.height]).toEqual([640, 140]);
    expect(hasInk(image)).toBe(true);
    expect(pixelAt(image, 2, 2)).toEqual({ red: 255, green: 255, blue: 255 });
    expect(alpha(image, 2, 2)).toBe(255);
  });

  it("rounds the pixel size as Excalidraw rounds it", async () => {
    const editor = harness.core.makeEditor();
    editor.load(
      encoder.encode(
        `{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":100.2,"height":50.3,"roundness":null}]}`,
      ),
    );
    const image = (await render(editor, ExportScope.Scene, true))!;
    // Math.round of 240.4 x 140.6
    expect([image.width, image.height]).toEqual([240, 141]);
  });

  it("leaves the paper transparent without a background", async () => {
    const image = (await render(twoRectangles(), ExportScope.Scene, false))!;

    expect(alpha(image, 2, 2)).toBe(0);
    expect(alpha(image, image.width / 2, image.height / 2)).toBe(0); // between the rectangles
    expect(hasInk(image)).toBe(true);
  });

  it("exports the Selection alone", async () => {
    const editor = twoRectangles();
    drag(editor, [[300, 125]]);
    expect(editor.selection()).toEqual(["b"]);

    const image = (await render(editor, ExportScope.Selection, true))!;
    // (100 + 2 * 10) x (50 + 2 * 10) at 2x
    expect([image.width, image.height]).toEqual([240, 140]);
    expect(hasInk(image)).toBe(true);

    expect(await render(twoRectangles(), ExportScope.Selection, true)).toBeUndefined();
  });

  it("makes the same export in Dark mode, on white paper", async () => {
    const editor = twoRectangles();
    const light = (await render(editor, ExportScope.Scene, true))!;

    editor.setDarkMode(true);
    const dark = (await render(editor, ExportScope.Scene, true))!;

    expect([dark.width, dark.height]).toEqual([light.width, light.height]);
    expect(dark.rgba).toEqual(light.rgba);
    expect(pixelAt(dark, 2, 2)).toEqual({ red: 255, green: 255, blue: 255 });
  });

  it("is a PNG", async () => {
    const png = (await exportPng(
      harness.device,
      harness.font,
      twoRectangles(),
      ExportScope.Scene,
      false,
    ))!;

    expect([...png.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
    const bitmap = await createImageBitmap(new Blob([png as Uint8Array<ArrayBuffer>]));
    expect(bitmap.width).toBe(640);
  });

  it("holds straight colour where the paper is transparent", async () => {
    const editor = harness.core.makeEditor();
    editor.load(
      encoder.encode(
        `{"type":"excalidraw","version":2,"elements":[{"id":"a","type":"rectangle","x":0,"y":0,"width":100,"height":50,"strokeColor":"#000000","backgroundColor":"#ff0000","fillStyle":"solid","opacity":50,"roughness":0,"roundness":null}]}`,
      ),
    );
    const image = (await render(editor, ExportScope.Scene, false))!;
    const middle = (image.height / 2) * image.width + image.width / 2;
    const [red, green, blue, a] = image.rgba.subarray(middle * 4, middle * 4 + 4);
    expect(a).toBeGreaterThan(110);
    expect(a).toBeLessThan(145);
    expect(red).toBeGreaterThan(240); // premultiplied it would be about half
    expect([green, blue]).toEqual([0, 0]);
  });
});
