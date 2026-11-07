import init, {
  Editor as WasmEditor,
  type InitInput,
  type ViewportState,
} from "../../generated/excalidraw_native_web.js";
import { type TextMeasurer } from "../text/Excalifont";
import {
  type GeometryBuffers,
  type GeometrySource,
  MemoryViews,
  readGeometry,
} from "../rendering/GeometryBuffers";

export type { ViewportState };

/** The wasm core, loaded. */
export interface Core {
  memory: WebAssembly.Memory;
  makeEditor(): Editor;
}

/**
 * Loads the wasm from a URL, bytes or a compiled module. Editors measure text
 * with `measurer`; without one, labels keep the lines saved in the file.
 */
export async function loadCore(
  source: InitInput | Promise<InitInput>,
  measurer?: TextMeasurer,
): Promise<Core> {
  const exports = await init({ module_or_path: source });
  const views = new MemoryViews(exports.memory);
  return {
    memory: exports.memory,
    makeEditor() {
      const editor = new WasmEditor();
      if (measurer) editor.setTextMeasurer(measurer);
      return new Editor(editor, views);
    },
  };
}

/**
 * The core's editor, mirroring the Swift client's view of `Editor`. Free it
 * when done: wasm memory is not garbage collected.
 */
export class Editor {
  constructor(
    private readonly inner: WasmEditor,
    private readonly views: MemoryViews,
  ) {}

  /** Replaces the Scene with the Drawing in `bytes`; throws if it is not one. */
  load(bytes: Uint8Array): void {
    this.inner.load(bytes);
  }

  /** The Scene as `.excalidraw` file content. */
  save(): Uint8Array {
    return this.inner.save();
  }

  elementCount(): number {
    return this.inner.elementCount();
  }

  viewport(): ViewportState {
    return this.inner.viewport();
  }

  setViewport(viewport: ViewportState): void {
    this.inner.setViewport(viewport);
  }

  /** Reports the Canvas size in points; call on start and on every resize. */
  setViewSize(width: number, height: number): void {
    this.inner.setViewSize(width, height);
  }

  setDarkMode(on: boolean): void {
    this.inner.setDarkMode(on);
  }

  /** The Canvas colour as `0xRRGGBBAA`. */
  canvasColor(): number {
    return this.inner.canvasColor();
  }

  geometry(): Geometry {
    return new Geometry(this.inner.geometry(), this.views);
  }

  free(): void {
    this.inner.free();
  }
}

/** The core's geometry, read in place from wasm memory. Free it when done. */
export class Geometry implements GeometrySource {
  constructor(
    private readonly inner: ReturnType<WasmEditor["geometry"]>,
    private readonly views: MemoryViews,
  ) {}

  generation(): number {
    return this.inner.generation();
  }

  withBuffers<R>(body: (buffers: GeometryBuffers) => R): R {
    return readGeometry(this.inner, this.views, body);
  }

  free(): void {
    this.inner.free();
  }
}
