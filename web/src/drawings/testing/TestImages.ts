import { type RenderedImage, bytesPerPixel } from "../rendering/RenderedImage";

/** Image files for browser tests, made in memory so that no binary fixtures are needed. */
export const red = [255, 0, 0, 255];
export const green = [0, 255, 0, 255];
export const blue = [0, 0, 255, 255];
export const yellow = [255, 255, 0, 255];

/** A PNG of `width` x `height` pixels, each given by `color(x, y)` as non-premultiplied RGBA. */
export async function png(
  width: number,
  height: number,
  color: (x: number, y: number) => number[],
): Promise<Uint8Array> {
  const pixels = new Uint8ClampedArray(width * height * bytesPerPixel);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) pixels.set(color(x, y), (y * width + x) * bytesPerPixel);
  }
  const canvas = new OffscreenCanvas(width, height);
  canvas.getContext("2d")!.putImageData(new ImageData(pixels, width, height), 0, 0);
  return new Uint8Array(await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer());
}

/** Red top left, green top right, blue bottom left, yellow bottom right. */
export function quadrants(size = 40): Promise<Uint8Array> {
  return png(size, size, (x, y) => {
    if (x < size / 2) return y < size / 2 ? red : blue;
    return y < size / 2 ? green : yellow;
  });
}

/** `image` encoded again as `type` ("image/jpeg", "image/webp"). */
export async function reencoded(bytes: Uint8Array, type: string, quality = 1): Promise<Uint8Array> {
  const bitmap = await createImageBitmap(new Blob([bytes as Uint8Array<ArrayBuffer>]));
  const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
  canvas.getContext("2d")!.drawImage(bitmap, 0, 0);
  return new Uint8Array(await (await canvas.convertToBlob({ type, quality })).arrayBuffer());
}

/** Pixels no encoder can shrink: deterministic noise. */
export function noise(width: number, height: number): Promise<Uint8Array> {
  let state = 12345;
  return png(width, height, () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return [state >>> 24, (state >>> 16) & 255, (state >>> 8) & 255, 255];
  });
}

/** Red, green and blue at device pixel `(x, y)`. */
export function rgb(image: RenderedImage, x: number, y: number): number[] {
  const start = (y * image.width + x) * bytesPerPixel;
  return [image.rgba[start], image.rgba[start + 1], image.rgba[start + 2]];
}
