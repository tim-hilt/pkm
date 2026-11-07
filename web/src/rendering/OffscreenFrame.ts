import { type CanvasRenderer } from "./CanvasRenderer";
import { type RenderedImage, bytesPerPixel } from "./RenderedImage";

/** The format of offscreen frames: `RenderedImage` is RGBA. */
export const offscreenFormat: GPUTextureFormat = "rgba8unorm";

/**
 * Renders one frame of whatever `renderer` has been given into memory
 * instead of a canvas, with the same pipeline and 4x MSAA as the Canvas, and
 * waits for it. Thumbnails and tests use it; a port of the Swift
 * `renderOffscreen`.
 *
 * The renderer must have been made for `offscreenFormat`.
 *
 * @param pixelsPerPoint Device pixels per view point; the viewport's zoom is in points.
 */
export async function renderOffscreen(
  device: GPUDevice,
  renderer: CanvasRenderer,
  width: number,
  height: number,
  pixelsPerPoint = 1,
): Promise<RenderedImage> {
  if (renderer.format !== offscreenFormat) {
    throw new Error(`offscreen frames need a renderer for ${offscreenFormat}`);
  }
  const target = device.createTexture({
    size: [width, height],
    format: offscreenFormat,
    usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
  });
  renderer.renderFrame(target, pixelsPerPoint);

  // Rows of a texture copy start at multiples of 256 bytes.
  const rowBytes = width * bytesPerPixel;
  const paddedRowBytes = Math.ceil(rowBytes / 256) * 256;
  const readback = device.createBuffer({
    size: paddedRowBytes * height,
    usage: GPUBufferUsage.COPY_DST | GPUBufferUsage.MAP_READ,
  });
  const encoder = device.createCommandEncoder();
  encoder.copyTextureToBuffer(
    { texture: target },
    { buffer: readback, bytesPerRow: paddedRowBytes },
    [width, height],
  );
  device.queue.submit([encoder.finish()]);
  await readback.mapAsync(GPUMapMode.READ);

  const padded = new Uint8Array(readback.getMappedRange());
  const rgba = new Uint8Array(rowBytes * height);
  for (let row = 0; row < height; row++) {
    rgba.set(
      padded.subarray(row * paddedRowBytes, row * paddedRowBytes + rowBytes),
      row * rowBytes,
    );
  }
  readback.destroy();
  target.destroy();
  return { width, height, rgba };
}
