import type { Editor, ExportScope } from "../../core/Core";
import { CanvasRenderer } from "../rendering/CanvasRenderer";
import { offscreenFormat, renderOffscreen } from "../rendering/OffscreenFrame";
import type { RenderedImage } from "../rendering/RenderedImage";
import type { Excalifont } from "../text/Excalifont";
import { encodePng } from "../gallery/PngEncoder";

/** Device pixels per Scene unit, like Excalidraw's 2x export. */
export const exportScale = 2;

const paper = 0xffffffff;

/**
 * Rounded, not truncated or rounded up, as Excalidraw sizes its export canvas
 * (`exportToCanvas`), so that both export at the same size.
 */
const pixels = (length: number) => Math.max(1, Math.round(length * exportScale));

/**
 * Exports the Scene or the Selection as an image at 2x, drawn by the Canvas's
 * own WebGPU renderer offscreen (ADR-0002) from the core's export geometry and
 * frame (ADR-0008), on white paper. `undefined` if there is nothing to
 * export. A port of the Swift `PNGExport.render`.
 *
 * The paper is white in Dark mode too: Dark mode is display only (ADR-0010).
 */
export async function renderExport(
  device: GPUDevice,
  font: Excalifont,
  editor: Editor,
  scope: ExportScope,
): Promise<RenderedImage | undefined> {
  const exported = editor.export(scope);
  if (!exported) return undefined;
  const renderer = new CanvasRenderer(device, offscreenFormat, font);
  try {
    renderer.files = editor;
    renderer.paperColor = paper;
    renderer.setGeometry(exported.geometry);
    renderer.viewport = { originX: exported.x, originY: exported.y, zoom: 1 };
    return await renderOffscreen(
      device,
      renderer,
      pixels(exported.width),
      pixels(exported.height),
      exportScale,
    );
  } finally {
    renderer.dispose();
    exported.geometry.free();
  }
}

/** The export as PNG file content; `undefined` as for `renderExport`. */
export async function exportPng(
  device: GPUDevice,
  font: Excalifont,
  editor: Editor,
  scope: ExportScope,
): Promise<Uint8Array | undefined> {
  const image = await renderExport(device, font, editor, scope);
  return image && encodePng(image);
}
