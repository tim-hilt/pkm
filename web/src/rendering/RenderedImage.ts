/** A rendered frame in memory: 8-bit sRGB pixels, which is what the renderer writes and what reference PNGs store. */
export interface RenderedImage {
  width: number;
  height: number;
  /** RGBA, rows top to bottom, tightly packed. */
  rgba: Uint8Array;
}

export const bytesPerPixel = 4;

/** The colour channels at device pixel `(x, y)`. */
export function pixelAt(image: RenderedImage, x: number, y: number) {
  const start = (y * image.width + x) * bytesPerPixel;
  return { red: image.rgba[start], green: image.rgba[start + 1], blue: image.rgba[start + 2] };
}

/** Brightness of the red channel at `(x, y)`: enough to tell ink from paper in grey drawings. */
export function redAt(image: RenderedImage, x: number, y: number): number {
  return image.rgba[(y * image.width + x) * bytesPerPixel];
}
