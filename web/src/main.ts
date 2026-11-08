import { startApp } from "./App";
import { registerServiceWorker } from "./offline/registerServiceWorker";
import { loadCore } from "./core/Core";
import { OpfsStorage } from "./storage/OpfsStorage";
import { Excalifont } from "./text/Excalifont";
import { loadExcalifontWebFont } from "./text/WebFont";
import { acquireDevice, showUnsupportedPage } from "./WebGPUSupport";
import wasmUrl from "../generated/excalidraw_native_web_bg.wasm";
import fontUrl from "../generated/Excalifont-Regular.ttf";

const root = document.getElementById("app")!;

async function main(): Promise<void> {
  const device = await acquireDevice(navigator.gpu);
  if (!device) return showUnsupportedPage(root);

  // The font is loaded before the first frame: text is measured and drawn with it.
  const fontBytes = new Uint8Array(await (await fetch(fontUrl)).arrayBuffer());
  const font = new Excalifont(fontBytes);
  await loadExcalifontWebFont(fontBytes);
  const core = await loadCore(fetch(wasmUrl), font);
  await startApp(root, device, core, font, OpfsStorage.open());
}

registerServiceWorker();
void main();
