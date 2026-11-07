import type { ImageCodec, OutputImageType } from "./ImageImporter";

const blobOf = (bytes: Uint8Array) => new Blob([bytes as Uint8Array<ArrayBuffer>]);

/** The browser's decoders and encoders: `createImageBitmap` turns pictures upright by their EXIF orientation, canvases encode. */
export const browserImageCodec: ImageCodec = {
  async size(bytes) {
    const bitmap = await decode(bytes);
    if (!bitmap) return undefined;
    const size = { width: bitmap.width, height: bitmap.height };
    bitmap.close();
    return size;
  },

  async encode(bytes, maxSide, type: OutputImageType, quality) {
    const bitmap = await decode(bytes);
    if (!bitmap) return undefined;
    try {
      const factor = Math.min(1, maxSide / Math.max(bitmap.width, bitmap.height));
      const width = Math.max(1, Math.round(bitmap.width * factor));
      const height = Math.max(1, Math.round(bitmap.height * factor));
      const canvas = new OffscreenCanvas(width, height);
      const context = canvas.getContext("2d")!;
      if (type === "image/jpeg") {
        // JPEG has no transparency: what has none is shown on white.
        context.fillStyle = "#fff";
        context.fillRect(0, 0, width, height);
      }
      context.imageSmoothingQuality = "high";
      context.drawImage(bitmap, 0, 0, width, height);
      const blob = await canvas.convertToBlob({ type, quality });
      return { bytes: new Uint8Array(await blob.arrayBuffer()), width, height };
    } catch {
      return undefined;
    } finally {
      bitmap.close();
    }
  },
};

async function decode(bytes: Uint8Array): Promise<ImageBitmap | undefined> {
  try {
    return await createImageBitmap(blobOf(bytes), { imageOrientation: "from-image" });
  } catch {
    return undefined;
  }
}
