import { startApp } from "./App";
import { loadCore } from "./core/Core";
import { MemoryStorage } from "./storage/MemoryStorage";
import { OpfsStorage } from "./storage/OpfsStorage";
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
  // `?sample` opens the sample Drawing, in temporary storage: nothing of it is kept.
  if (new URLSearchParams(location.search).has("sample")) {
    const sample = new Uint8Array(await (await fetch(sampleUrl)).arrayBuffer());
    await startApp(root, device, core, font, {
      storage: new MemoryStorage(),
      persistent: false,
      sample,
    });
  } else {
    await startApp(root, device, core, font, { storage: OpfsStorage.open(), persistent: true });
  }
}

void main();
