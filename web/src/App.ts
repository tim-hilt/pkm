import { showCanvas } from "./drawings/CanvasView";
import { CloudBacking } from "./cloud/CloudBacking";
import { Appearance } from "./settings/Appearance";
import { applyChromeColors } from "./drawings/chrome/ChromeColors";
import { SettingsView } from "./settings/SettingsView";
import { DrawingSettings } from "./drawings/chrome/DrawingSettings";
import { copyInBrowser, DrawingExport, downloadInBrowser } from "./drawings/export/DrawingExport";
import { exportPng } from "./drawings/export/PngExport";
import type { Core } from "./core/Core";
import { DrawingStore } from "./drawings/gallery/DrawingStore";
import { Gallery } from "./drawings/gallery/Gallery";
import { GalleryView } from "./drawings/gallery/GalleryView";
import type { OpenDrawing } from "./drawings/gallery/OpenDrawing";
import { ThumbnailRenderer } from "./drawings/gallery/ThumbnailRenderer";
import { Thumbnails } from "./drawings/gallery/Thumbnails";
import type { FileStorage } from "./storage/FileStorage";
import { requestPersistenceOnFirstCall } from "./storage/PersistentStorage";
import type { Excalifont } from "./drawings/text/Excalifont";
import { browserHistory } from "./navigation/browserHistory";
import { Navigation } from "./navigation/Navigation";
import { Shell } from "./shell/Shell";
import { TaskFiles } from "./tasks/TaskFiles";
import { TasksModel } from "./tasks/TasksModel";
import { TasksView } from "./tasks/TasksView";

/**
 * The app: the shell with its five Contexts; the Tasks in their pane, and in
 * Drawings the Gallery, from which a Drawing is opened into the Canvas, and
 * back. Drawings are kept saved as they change (autosave), and when the page
 * goes to the background.
 */
export async function startApp(
  root: HTMLElement,
  device: GPUDevice,
  core: Core,
  font: Excalifont,
  storage: FileStorage,
): Promise<void> {
  const store = new DrawingStore(storage, core, requestPersistenceOnFirstCall(navigator.storage));
  const thumbnails = new Thumbnails(store, new ThumbnailRenderer(device, core, font));
  const gallery = await Gallery.start(store, core, thumbnails);

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

  const navigation = new Navigation(browserHistory(window), localStorage);
  const tasks = new TasksModel(await TaskFiles.open(core, storage));
  const shell = new Shell(navigation, new SettingsView(navigation, appearance, cloud, tasks));
  root.replaceChildren(shell.element);
  tasks.onSaved = () => cloud.tasksSaved();
  cloud.onTasksChanged(() => void tasks.syncDidChange());
  shell.pane("tasks").append(
    new TasksView(tasks, {
      open: (id) => navigation.openItem(id),
      back: () => navigation.back(),
      close: () => navigation.closeItem(),
    }).element,
  );
  const pane = shell.pane("drawings");

  let canvas: { drawing: OpenDrawing; stop: () => void } | undefined;
  let galleryView: GalleryView | undefined;

  const show = () => {
    const drawing = gallery.open;
    if (canvas && canvas.drawing !== drawing) {
      canvas.stop();
      canvas = undefined;
      // Gone without the address asking (deleted by Sync): its address goes too.
      if (!drawing) navigation.closeItem();
    }
    shell.setFocused(drawing !== undefined);
    // An opened Drawing (from the Gallery) gets its address.
    if (drawing && gallery.openId) navigation.openItem(gallery.openId);
    if (drawing) {
      galleryView?.dispose();
      galleryView = undefined;
      if (!canvas) {
        const shown = showCanvas(pane, device, drawing.editor, font, {
          edited: () => drawing.editorDidChange(),
          back: () => navigation.closeItem(),
          settings,
          appearance,
          export: new DrawingExport(drawing, {
            save: downloadInBrowser,
            copy: copyInBrowser,
            png: (scope) => exportPng(device, font, drawing.editor, scope),
          }),
        });
        drawing.onMerged = shown.refresh;
        canvas = { drawing, stop: shown.stop };
      }
    } else {
      if (!galleryView) {
        galleryView = new GalleryView(gallery, () => appearance.isDark, cloud);
        pane.replaceChildren(galleryView.element);
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
  // The address decides what is open: no Item (or another Context) is the Gallery and the
  // Tasks list, and an Item ID the Device does not have is dropped from the address.
  const followAddress = () => {
    shell.update();
    const { selected, item } = navigation;
    const task = selected === "tasks" ? item : undefined;
    const drawing = selected === "drawings" ? item : undefined;
    // The Title on top is empty: the address is put back, whatever the browser went to.
    const held = selected === "tasks" && tasks.isLeavingBlocked ? tasks.editing?.id : undefined;
    if (held !== undefined && held !== task) return navigation.openItem(held);
    tasks.openLinked(task);
    if (task !== undefined && !tasks.editing) return navigation.closeItem();
    if (drawing === undefined) void gallery.close();
    else if (gallery.openId === drawing) return;
    else if (gallery.drawings.some((entry) => entry.id === drawing))
      void gallery.openDrawing(drawing);
    else navigation.closeItem();
  };
  navigation.onChange = followAddress;
  followAddress();
  show();

  // The page may be gone before the delay of an autosave is over.
  // So may a pending Close of a Task, which is committed too.
  const background = () => {
    void gallery.open?.saveNow();
    tasks.commit();
  };
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") background();
  });
  addEventListener("pagehide", background);
}
