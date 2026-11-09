import { beforeAll, describe, expect, it } from "vitest";
import { type Core, type Editor, loadCore } from "../core/Core";
import { decodeImage } from "../rendering/ImageDecoder";
import { Excalifont } from "../text/Excalifont";
import { blue, green, noise, png, quadrants, red, reencoded } from "../testing/TestImages";
import { browserImageCodec } from "./BrowserImageCodec";
import { fileIdFor, maxImportFileSize, maxImportPixelSize, prepareImage } from "./ImageImporter";
import { insertImages } from "./ImageInsertion";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import fontUrl from "../../generated/Excalifont-Regular.ttf?url";

/**
 * The browser's own decoders and encoders behind `decodeImage`,
 * `prepareImage` and `insertImages`: ports of the Swift `ImageDecoderTests`,
 * `ImageImporterTests` and `InsertImagesTests` that need real pictures.
 */

let core: Core;
beforeAll(async () => {
  const font = new Excalifont(new Uint8Array(await (await fetch(fontUrl)).arrayBuffer()));
  core = await loadCore(fetch(wasmUrl), font);
});

/** The size of the picture in `bytes` as the browser decodes it, with no limit. */
async function sizeOf(bytes: Uint8Array) {
  const decoded = await decodeImage(bytes, 100_000);
  const size = decoded && [decoded.width, decoded.height];
  decoded?.bitmap.close();
  return size;
}

/** The RGBA bytes of a decoded picture's top left corner pixels, row by row, read back through a canvas. */
async function pixelsOf(bytes: Uint8Array, count: number) {
  const decoded = (await decodeImage(bytes))!;
  const canvas = new OffscreenCanvas(decoded.width, decoded.height);
  const context = canvas.getContext("2d", { willReadFrequently: true })!;
  context.drawImage(decoded.bitmap, 0, 0);
  decoded.bitmap.close();
  return context.getImageData(0, 0, count, 1).data;
}

describe("decodeImage", () => {
  it("decodes a PNG top-down with its size", async () => {
    const bytes = await png(2, 2, (x, y) => (x === 0 && y === 0 ? red : green));

    const decoded = (await decodeImage(bytes))!;

    expect([decoded.width, decoded.height]).toEqual([2, 2]);
    expect([decoded.bitmap.width, decoded.bitmap.height]).toEqual([2, 2]);
    decoded.bitmap.close();
    expect([...(await pixelsOf(bytes, 2))]).toEqual([...red, ...green]);
  });

  it("downscales large images keeping their proportions", async () => {
    const bytes = await png(300, 100, () => red);

    const decoded = (await decodeImage(bytes, 150))!;

    expect([decoded.width, decoded.height]).toEqual([150, 50]);
    expect([decoded.bitmap.width, decoded.bitmap.height]).toEqual([150, 50]);
    decoded.bitmap.close();
  });

  it("does not enlarge small images", async () => {
    expect(await sizeOf(await png(30, 10, () => red))).toEqual([30, 10]);
  });

  it("does not decode bytes that are no image, or a truncated file", async () => {
    expect(await decodeImage(new TextEncoder().encode("not an image"))).toBeUndefined();
    expect(await decodeImage(new Uint8Array())).toBeUndefined();
    expect(await decodeImage((await quadrants()).slice(0, 20))).toBeUndefined();
    expect(
      await decodeImage(new TextEncoder().encode("<svg xmlns='http://www.w3.org/2000/svg'/>")),
    ).toBeUndefined();
  });
});

describe("prepareImage with the browser's codec", () => {
  const prepare = (bytes: Uint8Array) => prepareImage(bytes, browserImageCodec);

  it("keeps an image within the limits exactly as it is", async () => {
    const bytes = await quadrants(40);

    const file = await prepare(bytes);

    expect(file?.bytes).toEqual(bytes);
    expect(file?.mimeType).toBe("image/png");
    expect([file?.width, file?.height]).toEqual([40, 40]);
    expect(file?.fileId).toBe(await fileIdFor(bytes));
  });

  it("downscales a wide image to the limit, as bytes that are what the size says", async () => {
    const bytes = await png(3000, 1000, (x) => (x < 1500 ? red : blue));

    const file = (await prepare(bytes))!;

    expect([file.width, file.height]).toEqual([1440, 480]);
    expect(await sizeOf(file.bytes)).toEqual([1440, 480]);
    expect(file.mimeType).toBe("image/png");
    expect(file.fileId, "the id is of the original, as on excalidraw.com").toBe(
      await fileIdFor(bytes),
    );
  });

  it("downscales a tall image to the limit on its height", async () => {
    const file = (await prepare(await png(500, 2880, () => green)))!;

    expect([file.width, file.height]).toEqual([250, 1440]);
  });

  it("shrinks an image over the size limit until it fits", async () => {
    // Noise does not compress: 1600 x 1000 pixels are 6 MiB as a PNG.
    const bytes = await noise(1600, 1000);
    expect(bytes.length).toBeGreaterThan(maxImportFileSize);

    const file = (await prepare(bytes))!;

    expect(file.bytes.length).toBeLessThanOrEqual(maxImportFileSize);
    expect(Math.max(file.width, file.height)).toBeLessThanOrEqual(maxImportPixelSize);
    expect(await sizeOf(file.bytes)).toEqual([file.width, file.height]);
    expect(file.width * 10, "proportions kept").toBe(file.height * 16);
  });

  it("recompresses a JPEG over the size limit as JPEG", async () => {
    const bytes = await reencoded(await noise(1400, 1400), "image/jpeg", 1);
    expect(bytes.length).toBeGreaterThan(maxImportFileSize);

    const file = (await prepare(bytes))!;

    expect(file.mimeType).toBe("image/jpeg");
    expect(file.bytes.length).toBeLessThanOrEqual(maxImportFileSize);
    expect(await sizeOf(file.bytes)).toEqual([file.width, file.height]);
  });

  it("keeps a small WebP, a format every browser reads", async () => {
    const bytes = await reencoded(await quadrants(40), "image/webp");

    const file = await prepare(bytes);

    expect(file?.mimeType).toBe("image/webp");
    expect(file?.bytes).toEqual(bytes);
  });

  it("does not import bytes that are no image", async () => {
    expect(await prepare(new TextEncoder().encode("not an image"))).toBeUndefined();
    expect(await prepare(new Uint8Array())).toBeUndefined();
  });
});

describe("insertImages with the browser's codec", () => {
  const makeEditor = (): Editor => {
    const editor = core.makeEditor();
    editor.setViewSize(800, 600);
    return editor;
  };

  it("stores a large image downscaled to the limit", async () => {
    const editor = makeEditor();

    await insertImages(editor, [await png(3000, 1000, () => red)]);

    const id = /"fileId": "([^"]+)"/.exec(new TextDecoder().decode(editor.save()))![1];
    expect(await sizeOf(editor.fileBytes(id)!)).toEqual([1440, 480]);
    editor.free();
  });

  it("inserts a picture as an Element with its natural size", async () => {
    const editor = makeEditor();

    await insertImages(editor, [await quadrants(40)], { x: 100, y: 80 });

    const saved = new TextDecoder().decode(editor.save());
    expect(saved).toMatch(/"x": 80\b/);
    expect(saved).toMatch(/"y": 60\b/);
    editor.free();
  });
});
