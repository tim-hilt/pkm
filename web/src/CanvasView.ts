import { CanvasRenderer } from "./rendering/CanvasRenderer";
import { SceneSync } from "./rendering/SceneSync";
import { cssCursor } from "./rendering/CursorStyle";
import { CanvasInput } from "./input/CanvasInput";
import { ZoomControls } from "./chrome/ZoomControls";
import { Toolbar } from "./chrome/Toolbar";
import { UndoRedoControls } from "./chrome/UndoRedoControls";
import type { Editor } from "./core/Core";
import type { Excalifont } from "./text/Excalifont";

/**
 * Puts a canvas that fills `root` and the chrome over it, draws `editor`'s
 * Scene on the canvas and forwards the input on it to the editor, as the Swift
 * `CanvasMetalView` does: after every input or command the Scene (if its
 * revision moved), the viewport, the cursor and the chrome are shown again.
 */
export function showCanvas(
  root: HTMLElement,
  device: GPUDevice,
  editor: Editor,
  font: Excalifont,
): void {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("webgpu")!;
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });

  const renderer = new CanvasRenderer(device, format, font);
  // On demand: one frame per request, however many changes asked for it.
  let scheduled = false;
  renderer.onNeedsDisplay = () => {
    if (scheduled) return;
    scheduled = true;
    requestAnimationFrame(() => {
      scheduled = false;
      renderer.renderFrame(context.getCurrentTexture(), window.devicePixelRatio);
    });
  };

  const sync = new SceneSync(editor, renderer);
  sync.showScene();

  let pointer: { x: number; y: number } | undefined;
  const zoomControls = new ZoomControls(editor, () => didReachEditor());
  const undoRedo = new UndoRedoControls(editor, () => didReachEditor());
  const toolbar = new Toolbar(editor, () => didReachEditor());
  const bottomLeft = document.createElement("div");
  bottomLeft.className = "bottom-left";
  bottomLeft.append(zoomControls.element, undoRedo.element);
  /** Shows whatever the editor did with an input or command. */
  const didReachEditor = () => {
    sync.sync();
    zoomControls.update();
    undoRedo.update();
    toolbar.update();
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
  new CanvasInput(canvas, {
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
    didReachEditor();
    renderer.onNeedsDisplay?.();
  };
  root.replaceChildren(canvas, toolbar.element, bottomLeft);
  new ResizeObserver(resize).observe(canvas);
  // The pixel ratio also changes on its own, when the window moves to a
  // screen of another scale; the canvas keeps its size then.
  const watchPixelRatio = () =>
    matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`).addEventListener(
      "change",
      () => {
        resize();
        watchPixelRatio();
      },
      { once: true },
    );
  watchPixelRatio();
  resize();
}
