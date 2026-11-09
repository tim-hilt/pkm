import { type RenderedImage, bytesPerPixel } from "../rendering/RenderedImage";

/** Why a rendered image does not match its reference. */
export interface ImageMismatch {
  /** One line for the test failure message. */
  summary: string;
  /** Pixels beyond the tolerance; 0 if the sizes differ and none were compared. */
  differingPixels: number;
  /** The reference with every differing pixel painted red; undefined if the sizes differ. */
  diff: RenderedImage | undefined;
}

/** How far a render may stray from its reference and still match; the same as the Swift tests'. */
export interface ImageTolerance {
  /**
   * Largest per-channel difference (0...255) of a pixel that still counts as
   * equal. Covers anti-aliasing coverage that varies slightly between GPUs;
   * on one machine renders are identical. Low enough that a 10 % change of
   * opacity or a colour one shade off fails.
   */
  channel: number;
  /**
   * Share of pixels allowed to differ by more than `channel`, e.g. MSAA samples
   * landing differently on a few edge pixels: 32 pixels of a 400 x 400
   * render. A moved, missing or recoloured element changes more than this.
   */
  pixelFraction: number;
}

export const standardTolerance: ImageTolerance = { channel: 8, pixelFraction: 0.0002 };

/** Undefined if `actual` matches `expected` within `tolerance`. */
export function mismatchBetween(
  actual: RenderedImage,
  expected: RenderedImage,
  tolerance: ImageTolerance = standardTolerance,
): ImageMismatch | undefined {
  if (actual.width !== expected.width || actual.height !== expected.height) {
    return {
      summary: `size is ${actual.width}x${actual.height}, reference is ${expected.width}x${expected.height}`,
      differingPixels: 0,
      diff: undefined,
    };
  }

  const diff = expected.rgba.slice();
  let differing = 0;
  for (let start = 0; start < diff.length; start += bytesPerPixel) {
    let largest = 0;
    for (let channel = 0; channel < bytesPerPixel; channel++) {
      largest = Math.max(
        largest,
        Math.abs(actual.rgba[start + channel] - expected.rgba[start + channel]),
      );
    }
    if (largest > tolerance.channel) {
      differing++;
      diff.set([255, 0, 0, 255], start);
    }
  }

  const allowed = Math.floor(tolerance.pixelFraction * expected.width * expected.height);
  if (differing <= allowed) return undefined;
  return {
    summary: `${differing} pixels differ by more than ${tolerance.channel}/255 (at most ${allowed} allowed)`,
    differingPixels: differing,
    diff: { width: expected.width, height: expected.height, rgba: diff },
  };
}
