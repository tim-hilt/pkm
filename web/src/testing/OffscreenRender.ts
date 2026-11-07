import { type Core, loadCore } from "../core/Core";
import { CanvasRenderer, type Viewport } from "../rendering/CanvasRenderer";
import { type GeometrySource } from "../rendering/GeometryBuffers";
import { offscreenFormat, renderOffscreen } from "../rendering/OffscreenFrame";
import { type RenderedImage } from "../rendering/RenderedImage";
import { acquireDevice } from "../WebGPUSupport";
import { TestScenes } from "./TestScenes";
import wasmUrl from "../../generated/excalidraw_native_web_bg.wasm?url";

/** The default viewport: the Scene's origin at the top left, no zoom. */
export const defaultViewport: Viewport = { originX: 0, originY: 0, zoom: 1 };

/**
 * What the browser tests render with: the GPU, the core and the scenes made
 * with it. Renders geometry the way the Canvas does, into memory instead of a
 * canvas; a port of the Swift `OffscreenRender`.
 */
export interface RenderHarness {
  device: GPUDevice;
  scenes: TestScenes;
  /** The core, to load Drawings into. */
  core: Core;
  /** A production renderer on the GPU. */
  makeRenderer(): CanvasRenderer;
  /**
   * Draws `geometry` seen through `viewport` into a `size` x `size` pixel
   * image, with the production renderer and its 4x MSAA. `pixelsPerPoint` is
   * the screen scale being simulated.
   */
  render(
    geometry: GeometrySource,
    viewport?: Viewport,
    options?: { pixelsPerPoint?: number; size?: number },
  ): Promise<RenderedImage>;
  /** Renders one frame of whatever `renderer` has been given. */
  draw(
    renderer: CanvasRenderer,
    options?: { pixelsPerPoint?: number; size?: number },
  ): Promise<RenderedImage>;
}

export async function makeRenderHarness(): Promise<RenderHarness> {
  const device = await acquireDevice(navigator.gpu);
  if (!device) throw new Error("these tests need WebGPU");
  const core = await loadCore(fetch(wasmUrl));
  const harness: RenderHarness = {
    device,
    core,
    scenes: new TestScenes(core),
    makeRenderer: () => new CanvasRenderer(device, offscreenFormat),
    async render(geometry, viewport = defaultViewport, options) {
      const renderer = harness.makeRenderer();
      renderer.setGeometry(geometry);
      renderer.viewport = viewport;
      return harness.draw(renderer, options);
    },
    draw(renderer, { pixelsPerPoint = 1, size = 400 } = {}) {
      return renderOffscreen(device, renderer, size, size, pixelsPerPoint);
    },
  };
  return harness;
}
