import { commands } from "vitest/browser";
import { type RenderedImage } from "../rendering/RenderedImage";
import { type ImageTolerance, standardTolerance } from "./ImageComparison";
import { type PngCodec, type ReferenceFiles, ReferenceStore } from "./ReferenceStore";

// The reference store in the browser: PNGs are encoded and decoded with the
// browser's own canvas, files are read and written by Node through commands.

declare module "vitest/browser" {
  interface BrowserCommands {
    readReferenceFile(path: string): Promise<string | null>;
    writeReferenceFile(path: string, base64: string): Promise<void>;
    removeReferenceFile(path: string): Promise<void>;
    isRecordingReferences(): Promise<boolean>;
  }
}

const base64 = {
  encode(bytes: Uint8Array): string {
    let binary = "";
    for (let i = 0; i < bytes.length; i += 0x8000) {
      binary += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
    }
    return btoa(binary);
  },
  decode: (text: string) => Uint8Array.from(atob(text), (c) => c.charCodeAt(0)),
};

const files: ReferenceFiles = {
  async read(path) {
    const content = await commands.readReferenceFile(path);
    return content === null ? undefined : base64.decode(content);
  },
  async write(path, bytes) {
    await commands.writeReferenceFile(path, base64.encode(bytes));
  },
  async remove(path) {
    await commands.removeReferenceFile(path);
  },
};

const codec: PngCodec = {
  async encode(image) {
    const canvas = new OffscreenCanvas(image.width, image.height);
    canvas
      .getContext("2d")!
      .putImageData(
        new ImageData(new Uint8ClampedArray(image.rgba), image.width, image.height),
        0,
        0,
      );
    return new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
  },
  async decode(bytes) {
    let bitmap: ImageBitmap;
    try {
      bitmap = await createImageBitmap(new Blob([bytes as BlobPart]), {
        premultiplyAlpha: "none",
        colorSpaceConversion: "none",
      });
    } catch {
      return undefined;
    }
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const context = canvas.getContext("2d")!;
    context.drawImage(bitmap, 0, 0);
    const { data } = context.getImageData(0, 0, bitmap.width, bitmap.height);
    return { width: bitmap.width, height: bitmap.height, rgba: new Uint8Array(data.buffer) };
  },
};

/**
 * Fails the current test unless `image` matches the reference image `name`
 * (web/references/<name>.png). Recording (`make web-record-references`)
 * writes missing or changed references instead; see
 * docs/web-renderer-reference-images.md.
 */
export async function expectMatchesReference(
  image: RenderedImage,
  name: string,
  tolerance: ImageTolerance = standardTolerance,
): Promise<void> {
  const recording = await commands.isRecordingReferences();
  const outcome = await new ReferenceStore(files, codec, recording).check(image, name, tolerance);
  if (outcome.status === "failed") throw new Error(outcome.message);
}
