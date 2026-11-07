import type { Core, Editor, Geometry } from "../core/Core";
import { CanvasRenderer } from "../rendering/CanvasRenderer";
import { offscreenFormat, renderOffscreen } from "../rendering/OffscreenFrame";
import type { RenderedImage } from "../rendering/RenderedImage";
import type { Excalifont } from "../text/Excalifont";
import type { ThumbnailPainter } from "./Thumbnails";

/** Pixels. 4:3, like a Gallery card, and enough for a card of 260 points on a 2x screen; kept small because many thumbnails are held in memory. */
export const thumbnailWidth = 512;
export const thumbnailHeight = 384;
/** Paper kept free around the Scene, in pixels. */
export const thumbnailMargin = 16;

/**
 * Draws thumbnails of Drawings for the Gallery with the Canvas's own WebGPU
 * renderer, offscreen: the whole Scene, framed like "zoom to fit" (never
 * enlarged past 100 %), on the Canvas's paper, white or, in dark mode, dark.
 * A port of the Swift `ThumbnailRenderer`.
 *
 * One renderer and one editor are kept for all thumbnails, as making them is
 * costly; the renderer holds on to the last Drawing's draws until the next one
 * replaces them. The Drawing is drawn one at a time (`Thumbnails` sees to it).
 */
export class ThumbnailRenderer implements ThumbnailPainter {
  private renderer: CanvasRenderer | undefined;
  private editor: Editor | undefined;
  private geometry: Geometry | undefined;

  constructor(
    private readonly device: GPUDevice,
    private readonly core: Core,
    private readonly font: Excalifont,
  ) {}

  /** `undefined` if `drawing` (a `.excalidraw` file's bytes) is no valid Drawing. */
  async render(drawing: Uint8Array, dark = false): Promise<RenderedImage | undefined> {
    const renderer = (this.renderer ??= new CanvasRenderer(
      this.device,
      offscreenFormat,
      this.font,
    ));
    const editor = (this.editor ??= this.core.makeEditor());
    editor.setDarkMode(dark);
    renderer.files = editor;
    renderer.isDark = dark;
    try {
      editor.load(drawing);
    } catch {
      return undefined;
    }
    // The core frames the Scene in a view as big as the thumbnail less its
    // margin; the margin is put back by moving the viewport.
    const margin = thumbnailMargin;
    editor.setViewSize(thumbnailWidth - 2 * margin, thumbnailHeight - 2 * margin);
    editor.zoomToFit();
    const { originX, originY, zoom } = editor.viewport();

    const previous = this.geometry;
    this.geometry = editor.geometry();
    renderer.setGeometry(this.geometry);
    previous?.free();
    renderer.paperColor = editor.canvasColor();
    renderer.viewport = {
      originX: originX - margin / zoom,
      originY: originY - margin / zoom,
      zoom,
    };
    return renderOffscreen(this.device, renderer, thumbnailWidth, thumbnailHeight, 1);
  }
}
