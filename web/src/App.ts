import { showCanvas } from "./CanvasView";
import { CloudBacking } from "./cloud/CloudBacking";
import { Appearance } from "./chrome/Appearance";
import { applyChromeColors } from "./chrome/ChromeColors";
import { SettingsView } from "./chrome/SettingsView";
import { DrawingSettings } from "./chrome/DrawingSettings";
import { DrawingExport, downloadInBrowser } from "./export/DrawingExport";
import { exportPng } from "./export/PngExport";
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

  // Cloud Backing Syncs while the tab is shown and online; the Gallery lists again what a step changed.
  const cloud = await CloudBacking.open(core, storage, (url, init) => fetch(url, init), store);
  cloud.setVisible(document.visibilityState === "visible");
  cloud.setOnline(navigator.onLine);
  store.onLocalChange = () => cloud.localChange();
  cloud.onSynced((names) => {
    // The open Drawing takes what Sync did to it before the Gallery lists again.
    void (gallery.open?.syncDidChange(names) ?? Promise.resolve()).then(() => gallery.reload());
  });
  document.addEventListener("visibilitychange", () =>
    cloud.setVisible(document.visibilityState === "visible"),
  );
  addEventListener("online", () => cloud.setOnline(true));
  addEventListener("offline", () => cloud.setOnline(false));
  const settings = new DrawingSettings(localStorage);
  const appearance = new Appearance(localStorage, matchMedia("(prefers-color-scheme: dark)"));
  const showChromeColors = () => applyChromeColors(document.documentElement, appearance.isDark);
  showChromeColors();

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
      if (!canvas) {
        const shown = showCanvas(root, device, drawing.editor, font, {
          edited: () => drawing.editorDidChange(),
          back: () => void gallery.close(),
          settings,
          appearance,
          export: new DrawingExport(drawing, {
            save: downloadInBrowser,
            png: (scope, background) => exportPng(device, font, drawing.editor, scope, background),
          }),
        });
        drawing.onMerged = shown.refresh;
        canvas = { drawing, stop: shown.stop };
      }
    } else {
      if (!galleryView) {
        galleryView = new GalleryView(
          gallery,
          () => appearance.isDark,
          new SettingsView(appearance, cloud),
        );
        root.replaceChildren(galleryView.element);
      }
      galleryView.update();
    }
  };
  // The Gallery's thumbnails are of the Canvas in the appearance shown.
  appearance.subscribe(() => {
    showChromeColors();
    galleryView?.appearanceChanged();
  });
  gallery.onChange = show;
  show();

  // The page may be gone before the delay of an autosave is over.
  const background = () => void gallery.open?.saveNow();
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") background();
  });
  addEventListener("pagehide", background);
}
