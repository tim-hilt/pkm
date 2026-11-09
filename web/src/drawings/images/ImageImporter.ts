import type { ImageFile } from "../../core/Core";

/**
 * Prepares a pasted or dropped image for the core (ADR-0003): the core only
 * stores bytes, so the client makes sure they are within the limits Excalidraw
 * has for imports (1440 px on the long side, 4 MiB) and in a format
 * excalidraw.com can read. A port of the Swift `ImageImporter`, over a codec
 * (the browser's, see `BrowserImageCodec`) that does the pixel work.
 */

/** Longest side of an imported image in pixels; larger pictures are downscaled. */
export const maxImportPixelSize = 1440;
/** Largest file in bytes; larger ones are recompressed, and shrunk if that is not enough. */
export const maxImportFileSize = 4 * 1024 * 1024;
/** Shrinking stops here: a picture this small has been lost anyway. */
const minPixelSize = 32;
/** JPEG qualities to try, best first, at each size. */
const jpegQualities = [0.85, 0.6, 0.4];

/** The formats every browser reads, which stay as they are when within the limits, so that the Drawing opens on excalidraw.com. */
export type WebImageType = "image/png" | "image/jpeg" | "image/gif" | "image/webp";
/** The formats an image is converted to. */
export type OutputImageType = "image/png" | "image/jpeg";

/** The pixel work, by the browser in production. */
export interface ImageCodec {
  /** The size of the picture as it is shown (turned upright by its EXIF orientation); `undefined` if the bytes are no image the codec reads. */
  size(bytes: Uint8Array): Promise<{ width: number; height: number } | undefined>;
  /**
   * The picture at most `maxSide` px on its long side (never enlarged),
   * upright, encoded as `type` (`quality` 0...1 matters to JPEG only);
   * `undefined` if that fails.
   */
  encode(
    bytes: Uint8Array,
    maxSide: number,
    type: OutputImageType,
    quality: number,
  ): Promise<{ bytes: Uint8Array; width: number; height: number } | undefined>;
}

/** The web format of `bytes` by their magic number; `undefined` for any other. */
export function sniffWebImageType(bytes: Uint8Array): WebImageType | undefined {
  const startsWith = (...signature: number[]) => signature.every((b, i) => bytes[i] === b);
  if (startsWith(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a)) return "image/png";
  if (startsWith(0xff, 0xd8, 0xff)) return "image/jpeg";
  if (startsWith(0x47, 0x49, 0x46, 0x38)) return "image/gif";
  // "RIFF", four bytes of size, "WEBP".
  if (startsWith(0x52, 0x49, 0x46, 0x46) && bytes[8] === 0x57 && bytes[9] === 0x45) {
    return bytes[10] === 0x42 && bytes[11] === 0x50 ? "image/webp" : undefined;
  }
  return undefined;
}

/**
 * What excalidraw.com calls the file: the SHA-1 of its bytes, as lower-case
 * hex. Taken of the original, before any downscaling, so that the same
 * picture is stored once however it was shrunk.
 */
export async function fileIdFor(bytes: Uint8Array): Promise<string> {
  const digest = await crypto.subtle.digest("SHA-1", bytes as Uint8Array<ArrayBuffer>);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

/**
 * The image in `data` ready to be inserted into a Drawing, or `undefined` if
 * the bytes are no image the codec understands.
 *
 * An image within the limits in a format browsers read is kept exactly as it
 * is. Any other is decoded, turned upright, downscaled and encoded again: as
 * JPEG if it is one, else as PNG (which keeps transparency). If that is still
 * too large it is shrunk by a quarter at a time (and JPEGs are compressed
 * harder) until it fits.
 */
export async function prepareImage(
  data: Uint8Array,
  codec: ImageCodec,
): Promise<ImageFile | undefined> {
  const natural = await codec.size(data);
  if (!natural || !(natural.width > 0 && natural.height > 0)) return undefined;
  const fileId = await fileIdFor(data);
  const type = sniffWebImageType(data);
  const longSide = Math.max(natural.width, natural.height);

  if (type && longSide <= maxImportPixelSize && data.length <= maxImportFileSize) {
    return { fileId, mimeType: type, bytes: data, width: natural.width, height: natural.height };
  }

  const output: OutputImageType = type === "image/jpeg" ? "image/jpeg" : "image/png";
  const qualities = output === "image/png" ? [1] : jpegQualities;
  for (
    let side = Math.min(longSide, maxImportPixelSize);
    side >= minPixelSize;
    side = Math.floor((side * 3) / 4)
  ) {
    for (const quality of qualities) {
      const encoded = await codec.encode(data, side, output, quality);
      if (encoded && encoded.bytes.length <= maxImportFileSize) {
        return {
          fileId,
          mimeType: output,
          bytes: encoded.bytes,
          width: encoded.width,
          height: encoded.height,
        };
      }
    }
  }
  return undefined;
}
