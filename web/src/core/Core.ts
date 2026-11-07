import init, {
  Cursor,
  Editor as WasmEditor,
  PointerButton,
  PointerKind,
  PointerPhase,
  Tool,
  type InitInput,
  ViewportState,
} from "../../generated/excalidraw_native_web.js";
import { type TextMeasurer } from "../text/Excalifont";
import {
  type GeometryBuffers,
  type GeometrySource,
  MemoryViews,
  readGeometry,
} from "../rendering/GeometryBuffers";

export { Cursor, PointerButton, PointerKind, PointerPhase, Tool };

/** What the Canvas shows: a plain value, as the wasm object is freed on reading. */
export interface Viewport {
  /** Scene point at the Canvas's top-left corner. */
  originX: number;
  originY: number;
  /** View points per Scene unit. */
  zoom: number;
}

/** Modifier keys held while an event happened. */
export interface Modifiers {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  /** Command on Apple platforms, the Windows key elsewhere. */
  meta: boolean;
}

/** One step in the life of a pointer; the core's `PointerEvent` (ADR-0006). */
export interface CorePointerEvent {
  id: number;
  phase: PointerPhase;
  /** Points from the Canvas's top-left corner, y down. */
  x: number;
  y: number;
  kind: PointerKind;
  button: PointerButton;
  /** `0..1`; `0.5` where the device does not report pressure. */
  pressure: number;
  /** Seconds on a monotonic clock. */
  timestamp: number;
  modifiers: Modifiers;
}

/** A scroll wheel or trackpad scroll: how far the content should move, in view points. */
export interface CoreScrollEvent {
  x: number;
  y: number;
  dx: number;
  dy: number;
  modifiers: Modifiers;
}

/** A trackpad pinch: the relative change of zoom since the previous event. */
export interface CoreMagnifyEvent {
  x: number;
  y: number;
  magnification: number;
}

const modifierBits = ({ shift, ctrl, alt, meta }: Modifiers) =>
  (shift ? 1 : 0) | (ctrl ? 2 : 0) | (alt ? 4 : 0) | (meta ? 8 : 0);

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

  viewport(): Viewport {
    const state = this.inner.viewport();
    const viewport = { originX: state.originX, originY: state.originY, zoom: state.zoom };
    state.free();
    return viewport;
  }

  setViewport({ originX, originY, zoom }: Viewport): void {
    const state = new ViewportState(originX, originY, zoom);
    this.inner.setViewport(state);
    state.free();
  }

  /** Differs from before whenever the Scene changed. */
  revision(): number {
    return this.inner.revision();
  }

  zoomIn(): void {
    this.inner.zoomIn();
  }

  zoomOut(): void {
    this.inner.zoomOut();
  }

  /** Back to 100 % around the middle of the view. */
  resetZoom(): void {
    this.inner.resetZoom();
  }

  zoomToFit(): void {
    this.inner.zoomToFit();
  }

  tool(): Tool {
    return this.inner.tool();
  }

  setTool(tool: Tool): void {
    this.inner.setTool(tool);
  }

  /** Whether the Tool stays after a shape is drawn. */
  toolLock(): boolean {
    return this.inner.toolLock();
  }

  setToolLock(locked: boolean): void {
    this.inner.setToolLock(locked);
  }

  /** Whether there is something to undo; changes after any input or command. */
  canUndo(): boolean {
    return this.inner.canUndo();
  }

  canRedo(): boolean {
    return this.inner.canRedo();
  }

  undo(): void {
    this.inner.undo();
  }

  redo(): void {
    this.inner.redo();
  }

  /** Differs from before whenever `overlay()` may: also on zoom, never on a pan. */
  overlayRevision(): number {
    return this.inner.overlayRevision();
  }

  /** What is drawn over the Scene, made for the current zoom. */
  overlay(): Geometry {
    return new Geometry(this.inner.overlay(), this.views);
  }

  /** Forwards a step of a finger, pen or mouse. */
  pointer(e: CorePointerEvent): void {
    this.inner.pointer(
      e.id,
      e.phase,
      e.x,
      e.y,
      e.kind,
      e.button,
      e.pressure,
      e.timestamp,
      modifierBits(e.modifiers),
    );
  }

  scroll(e: CoreScrollEvent): void {
    this.inner.scroll(e.x, e.y, e.dx, e.dy, modifierBits(e.modifiers));
  }

  magnify(e: CoreMagnifyEvent): void {
    this.inner.magnify(e.x, e.y, e.magnification);
  }

  /** The space bar went down (`true`) or up; the core pans with it held. */
  setPanKey(held: boolean): void {
    this.inner.setPanKey(held);
  }

  /** The cursor the core wants at the view point `(x, y)`. */
  cursor(x: number, y: number): Cursor {
    return this.inner.cursor(x, y);
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
