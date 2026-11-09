import type { Editor } from "../../core/Core";
import { browserImageCodec } from "./BrowserImageCodec";
import { type ImageCodec, prepareImage } from "./ImageImporter";

/** What inserting images needs of the editor. */
export type ImageInsertionTarget = Pick<Editor, "insertImages">;

/**
 * Puts the images in `sources` (the bytes of image files, in any format the
 * browser reads) on the Canvas: around `at` (in the Canvas's coordinates)
 * where they were dropped, else in the middle of the view. What is not an
 * image is skipped. Every route (file picker, drop, paste) ends here, as in
 * the Swift `CanvasImageInput`. Tells whether anything was inserted.
 */
export async function insertImages(
  editor: ImageInsertionTarget,
  sources: Uint8Array[],
  at?: { x: number; y: number },
  codec: ImageCodec = browserImageCodec,
): Promise<boolean> {
  const prepared = await Promise.all(sources.map((source) => prepareImage(source, codec)));
  const files = prepared.filter((file) => file !== undefined);
  if (files.length === 0) return false;
  editor.insertImages(files, at);
  return true;
}
