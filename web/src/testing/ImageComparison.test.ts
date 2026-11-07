import { describe, expect, it } from "vitest";
import { type RenderedImage, bytesPerPixel } from "../rendering/RenderedImage";
import { mismatchBetween } from "./ImageComparison";

/**
 * The comparison is the test oracle for every reference image, so its own
 * behaviour is pinned down with small synthetic images.
 */

const size = 100;

/** White paper with a grey vertical line `thickness` pixels wide at column `x`. */
function image({
  lineAt,
  thickness = 1,
  grey = 0,
  side = size,
}: { lineAt?: number; thickness?: number; grey?: number; side?: number } = {}): RenderedImage {
  const rgba = new Uint8Array(side * side * bytesPerPixel).fill(255);
  if (lineAt !== undefined) {
    for (let row = 0; row < side; row++) {
      for (let column = lineAt; column < lineAt + thickness; column++) {
        rgba.fill(
          grey,
          (row * side + column) * bytesPerPixel,
          (row * side + column) * bytesPerPixel + 3,
        );
      }
    }
  }
  return { width: side, height: side, rgba };
}

function withPixel(source: RenderedImage, x: number, y: number, grey: number): RenderedImage {
  const rgba = source.rgba.slice();
  rgba.fill(
    grey,
    (y * source.width + x) * bytesPerPixel,
    (y * source.width + x) * bytesPerPixel + 3,
  );
  return { ...source, rgba };
}

describe("image comparison", () => {
  it("matches identical images", () => {
    expect(mismatchBetween(image({ lineAt: 40 }), image({ lineAt: 40 }))).toBeUndefined();
  });

  it("tolerates faint anti-aliasing differences", () => {
    // Every pixel of the line is a few levels off, as after a GPU change.
    expect(
      mismatchBetween(image({ lineAt: 40, grey: 105 }), image({ lineAt: 40, grey: 100 })),
    ).toBeUndefined();
  });

  it("fails a slightly other colour", () => {
    // Ink (30) at 90 % opacity over paper is 52.
    expect(
      mismatchBetween(image({ lineAt: 40, grey: 52 }), image({ lineAt: 40, grey: 30 })),
    ).toBeDefined();
  });

  it("tolerates a few stray pixels", () => {
    // A handful of edge pixels flip, e.g. a different MSAA sample position.
    const actual = withPixel(withPixel(image({ lineAt: 40 }), 41, 0, 0), 41, 1, 0);
    expect(mismatchBetween(actual, image({ lineAt: 40 }))).toBeUndefined();
  });

  it("fails a moved line", () => {
    const mismatch = mismatchBetween(image({ lineAt: 41 }), image({ lineAt: 40 }));
    expect(mismatch?.differingPixels).toBe(2 * size);
  });

  it("fails a missing element", () => {
    expect(mismatchBetween(image(), image({ lineAt: 40, thickness: 2 }))).toBeDefined();
  });

  it("fails a different size, without a diff image", () => {
    const mismatch = mismatchBetween(image({ side: 50 }), image())!;
    expect(mismatch.diff).toBeUndefined();
    expect(mismatch.summary).toContain("50x50");
    expect(mismatch.summary).toContain("100x100");
  });

  it("marks differing pixels red in the diff image", () => {
    const diff = mismatchBetween(image({ lineAt: 41 }), image({ lineAt: 40 }))!.diff!;
    expect([diff.width, diff.height]).toEqual([size, size]);
    // Pixel (40, 10) lost its ink and (41, 10) gained some: both are flagged red.
    for (const x of [40, 41]) {
      const start = (10 * size + x) * bytesPerPixel;
      expect(Array.from(diff.rgba.subarray(start, start + 4)), `pixel ${x}`).toEqual([
        255, 0, 0, 255,
      ]);
    }
    // Untouched paper stays white, so red stands out.
    const paper = (10 * size + 5) * bytesPerPixel;
    expect(Array.from(diff.rgba.subarray(paper, paper + 3))).toEqual([255, 255, 255]);
  });
});
