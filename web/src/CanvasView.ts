import { CanvasRenderer } from "./rendering/CanvasRenderer";
import { SceneSync } from "./rendering/SceneSync";
import { cssCursor } from "./rendering/CursorStyle";
import { CanvasInput } from "./input/CanvasInput";
import { ZoomControls } from "./chrome/ZoomControls";
import { Toolbar } from "./chrome/Toolbar";
import { UndoRedoControls } from "./chrome/UndoRedoControls";
import { PropertiesPanel } from "./chrome/PropertiesPanel";
import { PhoneChrome } from "./chrome/PhoneChrome";
import { MainMenu } from "./chrome/MainMenu";
import type { DrawingSettings } from "./chrome/DrawingSettings";
import type { DrawingExport } from "./export/DrawingExport";
import type { Editor } from "./core/Core";
import type { Excalifont } from "./text/Excalifont";

/** What a shown Canvas tells its owner. */
export interface CanvasHandlers {
  /** Input or a command reached the editor (the Scene may have changed). */
  edited(): void;
  /** The user asked to go back to the Gallery. */
  back(): void;
  /** What the main menu downloads. */
  export: DrawingExport;
  settings: DrawingSettings;
}

/**
 * Puts a canvas that fills `root` and the chrome over it, draws `editor`'s
 * Scene on the canvas and forwards the input on it to the editor, as the Swift
 * `CanvasMetalView` does: after every input or command the Scene (if its
 * revision moved), the viewport, the cursor and the chrome are shown again.
 * Returns what stops it all, for when the Drawing is left.
 */
export function showCanvas(
  root: HTMLElement,
  device: GPUDevice,
  editor: Editor,
  font: Excalifont,
  handlers: CanvasHandlers,
): () => void {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("webgpu")!;
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });

  const renderer = new CanvasRenderer(device, format, font);
  // On demand: one frame per request, however many changes asked for it.
  let scheduled = false;
  let gone = false;
  renderer.onNeedsDisplay = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      if (gone) return;
      renderer.renderFrame(context.getCurrentTexture(), window.devicePixelRatio);
    });
  };

  const sync = new SceneSync(editor, renderer);
  sync.showScene();

  let pointer: { x: number; y: number } | undefined;
  const zoomControls = new ZoomControls(editor, () => didReachEditor());
  const undoRedo = new UndoRedoControls(editor, () => didReachEditor());
  const toolbar = new Toolbar(editor, () => didReachEditor());
  // Swatches are of the light Canvas until the Appearance setting brings Dark mode.
  const properties = new PropertiesPanel(
    editor,
    () => didReachEditor(),
    () => false,
  );
  const phone = new PhoneChrome(
    editor,
    () => didReachEditor(),
    () => false,
  );
  const mainMenu = new MainMenu({
    downloadDrawing: () => void handlers.export.downloadDrawing(),
    downloadPng: (scope) =>
      void handlers.export.downloadPng(scope, handlers.settings.exportBackground),
    hasSelection: () => editor.selection().length > 0,
    exportBackground: () => handlers.settings.exportBackground,
    setExportBackground: (on) => (handlers.settings.exportBackground = on),
    pencilOnlyDrawing: () => handlers.settings.pencilOnlyDrawing(editor),
    setPencilOnlyDrawing: (on) => {
      handlers.settings.setPencilOnlyDrawing(editor, on);
      didReachEditor();
    },
  });
  const back = document.createElement("button");
  back.type = "button";
  back.className = "back-to-gallery";
  back.textContent = "‹ Drawings";
  back.setAttribute("aria-label", "Back to Drawings");
  back.addEventListener("click", () => handlers.back());
  const bottomLeft = document.createElement("div");
  bottomLeft.className = "bottom-left";
  bottomLeft.append(zoomControls.element, undoRedo.element);
  /** Shows whatever the editor did with an input or command. */
  const didReachEditor = () => {
    sync.sync();
    handlers.edited();
    zoomControls.update();
    undoRedo.update();
    toolbar.update();
    properties.update();
    phone.update();
    mainMenu.update();
    // What a press would do may change with any input or command (a Tool,
    // the space bar, a drag).
    if (pointer) canvas.style.cursor = cssCursor(editor.cursor(pointer.x, pointer.y));
  };
  const send =
    <A extends unknown[]>(forward: (...args: A) => void) =>
    (...args: A) => {
      forward(...args);
      didReachEditor();
    };
  const input = new CanvasInput(canvas, {
    pointer: send((e) => {
      pointer = { x: e.x, y: e.y };
      editor.pointer(e);
    }),
    scroll: send((e) => editor.scroll(e)),
    magnify: send((e) => editor.magnify(e)),
    setPanKey: send((held) => editor.setPanKey(held)),
    hover: (x, y) => {
      pointer = { x, y };
      canvas.style.cursor = cssCursor(editor.cursor(x, y));
    },
  });

  const resize = () => {
    const ratio = window.devicePixelRatio;
    canvas.width = Math.max(1, Math.round(canvas.clientWidth * ratio));
    canvas.height = Math.max(1, Math.round(canvas.clientHeight * ratio));
    editor.setViewSize(canvas.clientWidth, canvas.clientHeight);
    renderer.paperColor = editor.canvasColor();
    properties.place(canvas.clientWidth, canvas.clientHeight, toolbar.element.offsetWidth);
    phone.place(canvas.clientWidth);
    didReachEditor();
    renderer.onNeedsDisplay?.();
  };
  root.replaceChildren(
    canvas,
    back,
    toolbar.element,
    properties.element,
    phone.element,
    mainMenu.element,
    bottomLeft,
  );
  const resizing = new ResizeObserver(resize);
  resizing.observe(canvas);
  // The pixel ratio also changes on its own, when the window moves to a
  // screen of another scale; the canvas keeps its size then.
  let stopWatchingPixelRatio = () => {};
  const watchPixelRatio = () => {
    const query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`);
    const changed = () => {
      resize();
      watchPixelRatio();
    };
    query.addEventListener("change", changed, { once: true });
    stopWatchingPixelRatio = () => query.removeEventListener("change", changed);
  };
  watchPixelRatio();
  resize();

  return () => {
    input.dispose();
    mainMenu.dispose();
    resizing.disconnect();
    stopWatchingPixelRatio();
    gone = true;
    renderer.dispose();
    sync.dispose();
    context.unconfigure();
    root.replaceChildren();
  };
}
