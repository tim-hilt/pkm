import { showCanvas } from "./CanvasView";
import { loadCore } from "./core/Core";
import { Excalifont } from "./text/Excalifont";
import { acquireDevice, showUnsupportedPage } from "./WebGPUSupport";
import wasmUrl from "../generated/excalidraw_native_web_bg.wasm";
import fontUrl from "../generated/Excalifont-Regular.ttf";
import sampleUrl from "../generated/sample.excalidraw";

const root = document.getElementById("app")!;

async function main(): Promise<void> {
  const device = await acquireDevice(navigator.gpu);
  if (!device) return showUnsupportedPage(root);

  // The font is loaded before the first frame: text is measured and drawn with it.
  const font = new Excalifont(new Uint8Array(await (await fetch(fontUrl)).arrayBuffer()));
  const core = await loadCore(fetch(wasmUrl), font);
  const editor = core.makeEditor();
  // `?sample` opens the sample Drawing, in temporary storage (nothing is saved yet).
  if (new URLSearchParams(location.search).has("sample")) {
    editor.load(new Uint8Array(await (await fetch(sampleUrl)).arrayBuffer()));
  }
  showCanvas(root, device, editor, font);
}

void main();
