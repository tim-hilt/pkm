import { showCanvas } from "./CanvasView";
import type { Core } from "./core/Core";
import { DrawingStore } from "./gallery/DrawingStore";
import { Gallery } from "./gallery/Gallery";
import { GalleryView } from "./gallery/GalleryView";
import type { OpenDrawing } from "./gallery/OpenDrawing";
import { ThumbnailRenderer } from "./gallery/ThumbnailRenderer";
import { Thumbnails } from "./gallery/Thumbnails";
import type { FileStorage } from "./storage/FileStorage";
import { requestPersistenceOnFirstCall } from "./storage/PersistentStorage";
import type { Excalifont } from "./text/Excalifont";

export interface AppOptions {
  /** Where the Drawings are kept. */
  storage: FileStorage;
  /** Whether the storage is the browser's own, which the first save asks to keep (not so for temporary storage). */
  persistent: boolean;
  /** A Drawing to put in the storage and open at the start. */
  sample?: Uint8Array;
}

/**
 * The app: the Gallery, from which a Drawing is opened into the Canvas, and
 * back. Drawings are kept saved as they change (autosave), and when the page
 * goes to the background.
 */
export async function startApp(
  root: HTMLElement,
  device: GPUDevice,
  core: Core,
  font: Excalifont,
  { storage, persistent, sample }: AppOptions,
): Promise<void> {
  const store = new DrawingStore(
    storage,
    persistent ? requestPersistenceOnFirstCall(navigator.storage) : undefined,
  );
  const thumbnails = new Thumbnails(store, new ThumbnailRenderer(device, core, font));
  const gallery = await Gallery.start(store, core, thumbnails);
  if (sample) await gallery.openDrawing(await store.add(sample, "Sample"));

  let canvas: { drawing: OpenDrawing; stop: () => void } | undefined;
  let galleryView: GalleryView | undefined;

  const show = () => {
    const drawing = gallery.open;
    if (canvas && canvas.drawing !== drawing) {
      canvas.stop();
      canvas = undefined;
    }
    if (drawing) {
      galleryView?.dispose();
      galleryView = undefined;
      canvas ??= {
        drawing,
        stop: showCanvas(root, device, drawing.editor, font, {
          edited: () => drawing.editorDidChange(),
          back: () => void gallery.close(),
        }),
      };
    } else {
      // Thumbnails are of the light Canvas until the Appearance setting brings Dark mode.
      if (!galleryView) {
        galleryView = new GalleryView(gallery, () => false);
        root.replaceChildren(galleryView.element);
      }
      galleryView.update();
    }
  };
  gallery.onChange = show;
  show();

  // The page may be gone before the delay of an autosave is over.
  const background = () => void gallery.open?.saveNow();
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") background();
  });
  addEventListener("pagehide", background);
}
