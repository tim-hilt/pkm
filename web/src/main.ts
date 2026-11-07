import { CanvasRenderer } from "./rendering/CanvasRenderer";
import { type Editor, loadCore } from "./core/Core";
import { acquireDevice, showUnsupportedPage } from "./WebGPUSupport";
import wasmUrl from "../generated/excalidraw_native_web_bg.wasm";
import sampleUrl from "../generated/sample.excalidraw";

const root = document.getElementById("app")!;

async function main(): Promise<void> {
  const device = await acquireDevice(navigator.gpu);
  if (!device) return showUnsupportedPage(root);

  const core = await loadCore(fetch(wasmUrl));
  const editor = core.makeEditor();
  // `?sample` opens the sample Drawing, in temporary storage (nothing is saved yet).
  if (new URLSearchParams(location.search).has("sample")) {
    editor.load(new Uint8Array(await (await fetch(sampleUrl)).arrayBuffer()));
  }
  showCanvas(root, device, editor);
}

/** Puts a canvas that fills the window into `root` and draws `editor`'s Scene on it. */
function showCanvas(root: HTMLElement, device: GPUDevice, editor: Editor): void {
  const canvas = document.createElement("canvas");
  root.replaceChildren(canvas);
  const context = canvas.getContext("webgpu")!;
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });

  const renderer = new CanvasRenderer(device, format);
  const geometry = editor.geometry();
  renderer.setGeometry(geometry);
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

  const resize = () => {
    const ratio = window.devicePixelRatio;
    canvas.width = Math.max(1, Math.round(canvas.clientWidth * ratio));
    canvas.height = Math.max(1, Math.round(canvas.clientHeight * ratio));
    editor.setViewSize(canvas.clientWidth, canvas.clientHeight);
    renderer.viewport = editor.viewport();
    renderer.paperColor = editor.canvasColor();
    renderer.onNeedsDisplay?.();
  };
  new ResizeObserver(resize).observe(canvas);
  resize();
}

void main();
