/**
 * Pixels decoded from an image file (ADR-0003: the core only stores the
 * file's bytes; decoding is the client's job, here the browser's own
 * decoders). A port of the Swift `ImageDecoder`.
 */
export interface DecodedImage {
  width: number;
  height: number;
  /** Premultiplied alpha, turned upright by the file's EXIF orientation like browsers show it. */
  bitmap: ImageBitmap;
}

/**
 * Longest side of a decoded image in pixels; larger pictures are downscaled
 * (Excalidraw itself limits imports to 1440). Bounds memory and keeps every
 * image within what any GPU takes as a texture.
 */
export const maxDecodedPixelSize = 4096;

/**
 * Decodes the PNG, JPEG, GIF, WebP... in `bytes`. `undefined` if the bytes are
 * not an image the browser understands (SVG, for one).
 */
export async function decodeImage(
  bytes: Uint8Array,
  maxPixelSize = maxDecodedPixelSize,
): Promise<DecodedImage | undefined> {
  try {
    const blob = new Blob([bytes as Uint8Array<ArrayBuffer>]);
    const options: ImageBitmapOptions = {
      premultiplyAlpha: "premultiply",
      imageOrientation: "from-image",
      colorSpaceConversion: "default",
    };
    const full = await createImageBitmap(blob, options);
    const longSide = Math.max(full.width, full.height);
    if (full.width === 0 || full.height === 0) return undefined;
    if (longSide <= maxPixelSize) return { width: full.width, height: full.height, bitmap: full };
    const factor = maxPixelSize / longSide;
    const resizeWidth = Math.max(1, Math.round(full.width * factor));
    const resizeHeight = Math.max(1, Math.round(full.height * factor));
    const smaller = await createImageBitmap(full, {
      ...options,
      resizeWidth,
      resizeHeight,
      resizeQuality: "high",
    });
    full.close();
    return { width: resizeWidth, height: resizeHeight, bitmap: smaller };
  } catch {
    return undefined;
  }
}
