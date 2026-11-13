import init, {
  Alignment,
  Cursor,
  Distribution,
  Edges,
  Editor as WasmEditor,
  ExportScope,
  FillPattern,
  FontSize,
  ImageFile as WasmImageFile,
  LineStyle,
  PointerButton,
  PointerKind,
  PointerPhase,
  Planning,
  Sloppiness,
  StrokeWidth,
  TaskStore,
  TextAlign,
  Tool,
  ZOrder,
  darkModeColor,
  isValidTag,
  stripForbiddenTagCharacters,
  areSameDrawing,
  claimItemIds,
  drawingItemId,
  drawingWithItemId,
  newItemId,
  type InitInput,
  ViewportState,
  SyncRun,
  SyncSchedule,
  WebDavProtocol,
} from "../../generated/pkm_web.js";
import { type TextMeasurer } from "../drawings/text/Excalifont";
import {
  type GeometryBuffers,
  type GeometrySource,
  MemoryViews,
  readGeometry,
} from "../drawings/rendering/GeometryBuffers";

export {
  Alignment,
  Cursor,
  Distribution,
  Edges,
  ExportScope,
  FillPattern,
  FontSize,
  LineStyle,
  PointerButton,
  PointerKind,
  PointerPhase,
  Planning,
  Sloppiness,
  StrokeWidth,
  TaskStore,
  TextAlign,
  Tool,
  ZOrder,
  darkModeColor,
  isValidTag,
  stripForbiddenTagCharacters,
};

/** The sections of the Properties panel that apply. */
export interface Sections {
  strokeColor: boolean;
  backgroundColor: boolean;
  fillStyle: boolean;
  strokeWidth: boolean;
  strokeStyle: boolean;
  sloppiness: boolean;
  edges: boolean;
  opacity: boolean;
  text: boolean;
  /** Align: two or more groups or Elements are selected. */
  align: boolean;
  /** Distribute: three or more are. */
  distribute: boolean;
}

/** What the Properties panel shows; `undefined` where the Selection's Elements differ. */
export interface Properties {
  strokeColor?: string;
  backgroundColor?: string;
  fillStyle?: FillPattern;
  strokeWidth?: StrokeWidth;
  strokeStyle?: LineStyle;
  sloppiness?: Sloppiness;
  edges?: Edges;
  /** 0 to 100. */
  opacity?: number;
  fontSize?: FontSize;
  textAlign?: TextAlign;
  sections: Sections;
}

/** One property to set, on the Selection and as the style of the next Element. */
export type Property =
  | { strokeColor: string }
  | { backgroundColor: string }
  | { fillStyle: FillPattern }
  | { strokeWidth: StrokeWidth }
  | { strokeStyle: LineStyle }
  | { sloppiness: Sloppiness }
  | { edges: Edges }
  | { opacity: number }
  | { fontSize: FontSize }
  | { textAlign: TextAlign };

/** What a PNG export shows: the geometry of it and the rectangle of the Scene, in Scene units, that the image is of. Free the geometry when done. */
export interface ImageExport {
  geometry: Geometry;
  x: number;
  y: number;
  width: number;
  height: number;
}

/** What the Canvas shows: a plain value, as the wasm object is freed on reading. */
export interface Viewport {
  /** Scene point at the Canvas's top-left corner. */
  originX: number;
  originY: number;
  /** View points per Scene unit. */
  zoom: number;
}

/** Which Context menu the core says a long-press opens; for an Element, what Group and Ungroup would do. */
export type ContextMenuKind =
  | { element: true; canGroup: boolean; canUngroup: boolean }
  | { element: false };

/** The text view to show over the Canvas while a text is edited, in Canvas points. */
export interface TextEditor {
  /** The text as typed: what the text view starts with. */
  text: string;
  /** The top left corner of the box, from the Canvas's top left. */
  x: number;
  y: number;
  width: number;
  height: number;
  /** In view points: the Scene's times the zoom. */
  fontSize: number;
  /** Relative to the font size. */
  lineHeight: number;
  fontFamily: number;
  align: TextAlign;
  /** `0xRRGGBBAA`. */
  color: number;
  /** `0..1`. */
  opacity: number;
  /** Clockwise, in radians. */
  angle: number;
  zoom: number;
  /** Whether the text wraps to the width of the box instead of growing. */
  fixedWidth: boolean;
}

/** Modifier keys held while an event happened. */
export interface Modifiers {
  shift: boolean;
  ctrl: boolean;
  alt: boolean;
  /** Command on Apple platforms, the Windows key elsewhere. */
  meta: boolean;
}

/** An image as the core stores it (ADR-0003): the bytes of the file as they are to be saved, and the size of the picture they show. */
export interface ImageFile {
  /** What the file is called in the Drawing; the same for the same content. */
  fileId: string;
  mimeType: string;
  bytes: Uint8Array;
  width: number;
  height: number;
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
  /** Whether the two Drawing files are the same Drawing: they share Elements, or both are empty. */
  areSameDrawing(a: Uint8Array, b: Uint8Array): boolean;
  /** The Item ID of the Drawing file, `undefined` if it holds none (or is no Drawing). */
  drawingItemId(bytes: Uint8Array): string | undefined;
  /** The Drawing file with `itemId` as its Item ID; throws if it is no Drawing or `itemId` is no Item ID. */
  drawingWithItemId(bytes: Uint8Array, itemId: string): Uint8Array;
  /** A new Item ID. */
  newItemId(): string;
  /**
   * The new Item IDs that Items need, given the ones they hold now, oldest
   * first (`""`: holds none): `""` where the Item keeps its own, a new Item ID
   * where it has none or the same as an older Item.
   */
  claimItemIds(oldestFirst: string[]): string[];
  /** The WebDAV protocol for a server's Drawings folder; throws if the URL is not https. Free it when done. */
  webDav(url: string, username: string, password: string): WebDavProtocol;
  /** The WebDAV protocol for the server's Books folder; throws if the URL is not https. Free it when done. */
  webDavForBooks(url: string, username: string, password: string): WebDavProtocol;
  /** A Sync step over the Drawings, starting at `nowMs` (milliseconds since the epoch). Free it when done. */
  syncRun(nowMs: number): SyncRun;
  /** A Sync step over the Book Files, which are transferred but never merged. Free it when done. */
  syncRunForBooks(nowMs: number): SyncRun;
  /** A Sync step over the Books' state files, merged by the Books merge hook. Free it when done. */
  syncRunForBookStates(nowMs: number): SyncRun;
  /** When Sync steps run, in front with a step due at `now` (milliseconds on any clock). Free it when done. */
  syncSchedule(now: number): SyncSchedule;
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
    areSameDrawing: (a, b) => areSameDrawing(a, b),
    drawingItemId: (bytes) => drawingItemId(bytes),
    drawingWithItemId: (bytes, itemId) => drawingWithItemId(bytes, itemId),
    newItemId: () => newItemId(),
    claimItemIds: (oldestFirst) => claimItemIds(oldestFirst),
    webDav: (url, username, password) => new WebDavProtocol(url, username, password),
    webDavForBooks: (url, username, password) =>
      WebDavProtocol.newForBooks(url, username, password),
    syncRun: (nowMs) => new SyncRun(nowMs),
    syncRunForBooks: (nowMs) => SyncRun.newForBooks(nowMs),
    syncRunForBookStates: (nowMs) => SyncRun.newForBookStates(nowMs),
    syncSchedule: (now) => new SyncSchedule(now),
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

  /**
   * Takes in `bytes`, the Drawing as Sync rewrote its file, by merging them
   * Element by Element into the Scene: no undo step, and Selection, Text view
   * and viewport stay; an Element being dragged, drawn or edited keeps its
   * local version. Throws, changing nothing, if `bytes` are not a Drawing.
   */
  mergeSynced(bytes: Uint8Array): void {
    this.inner.mergeSynced(bytes);
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

  /** Frames all shown Elements if none is in view: once after opening. */
  revealContent(): void {
    this.inner.revealContent();
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

  /** Performs the Command the core binds to `key` (`"r"`, `"1"`, `"["`, `"backspace"`, `"escape"`, `"enter"`; the character typed without Shift) with `modifiers`, where `meta` is the primary modifier (⌘ or Ctrl, see `KeyboardShortcuts`); whether there is one. */
  pressKey(key: string, modifiers: Modifiers): boolean {
    return this.inner.pressKey(key, modifierBits(modifiers));
  }

  /** The clipboard text (Excalidraw's clipboard JSON) for copying the Selection; `undefined` with nothing selected. */
  copySelection(): string | undefined {
    return this.inner.copySelection();
  }

  /** Copies the Selection and deletes it (one undo step); `undefined`, and nothing deleted, with nothing selected. */
  cutSelection(): string | undefined {
    return this.inner.cutSelection();
  }

  /** Pastes `text` if it is Excalidraw clipboard JSON, around `at` (view points) or in the middle of the view; whether it was. */
  paste(text: string, at?: { x: number; y: number }): boolean {
    return at ? this.inner.pasteAt(text, at.x, at.y) : this.inner.paste(text);
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

  /** A mouse or pen hovers at the view point `(x, y)`, no button down, with `modifiers` held. */
  hover(x: number, y: number, kind: PointerKind, modifiers: Modifiers): void {
    this.inner.hover(x, y, kind, modifierBits(modifiers));
  }

  /** Nothing hovers over the Canvas any more. */
  endHover(): void {
    this.inner.endHover();
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

  /** The ids of the selected Elements. */
  selection(): string[] {
    return this.inner.selection();
  }

  /** Puts `images` in the middle of the view, or around `at` (view points) where they were dropped; one undo step, and they become the Selection. */
  insertImages(images: ImageFile[], at?: { x: number; y: number }): void {
    const files = images.map(
      (i) => new WasmImageFile(i.fileId, i.mimeType, i.bytes, i.width, i.height),
    );
    if (at) this.inner.insertImagesAt(files, at.x, at.y);
    else this.inner.insertImages(files);
  }

  /** The bytes of the image file `id` as stored in the Drawing; `undefined` if there is none. */
  fileBytes(id: string): Uint8Array | undefined {
    return this.inner.fileBytes(id);
  }

  /** What to rasterise for a PNG of `scope`; `undefined` if there is nothing to export. */
  export(scope: ExportScope): ImageExport | undefined {
    const inner = this.inner.export(scope);
    if (!inner) return undefined;
    const { x, y, width, height } = inner;
    const geometry = new Geometry(inner.takeGeometry()!, this.views);
    inner.free();
    return { geometry, x, y, width, height };
  }

  /** Whether Pencil-only drawing is on; the first Pencil input may turn it on, so read it after input. */
  pencilOnlyDrawing(): boolean {
    return this.inner.pencilOnlyDrawing();
  }

  /** The user's choice or the stored one; Pencil input no longer turns it on by itself after that. */
  setPencilOnlyDrawing(on: boolean): void {
    this.inner.setPencilOnlyDrawing(on);
  }

  /** Whether the chrome shows the Properties panel: a drawing Tool is armed, text is edited or something is selected. */
  showsProperties(): boolean {
    return this.inner.showsProperties();
  }

  /** What the Properties panel shows; read it after every input or command. */
  properties(): Properties {
    const shown = this.inner.properties();
    const sections = shown.sections;
    const result: Properties = {
      strokeColor: shown.strokeColor,
      backgroundColor: shown.backgroundColor,
      fillStyle: shown.fillStyle,
      strokeWidth: shown.strokeWidth,
      strokeStyle: shown.strokeStyle,
      sloppiness: shown.sloppiness,
      edges: shown.edges,
      opacity: shown.opacity,
      fontSize: shown.fontSize,
      textAlign: shown.textAlign,
      sections: {
        strokeColor: sections.strokeColor,
        backgroundColor: sections.backgroundColor,
        fillStyle: sections.fillStyle,
        strokeWidth: sections.strokeWidth,
        strokeStyle: sections.strokeStyle,
        sloppiness: sections.sloppiness,
        edges: sections.edges,
        opacity: sections.opacity,
        text: sections.text,
        align: sections.align,
        distribute: sections.distribute,
      },
    };
    sections.free();
    shown.free();
    return result;
  }

  /** Sets a property on the Selection (one undo step) and as the style of the next Element. */
  setProperty(property: Property): void {
    if ("strokeColor" in property) this.inner.setStrokeColor(property.strokeColor);
    else if ("backgroundColor" in property) this.inner.setBackgroundColor(property.backgroundColor);
    else if ("fillStyle" in property) this.inner.setFillStyle(property.fillStyle);
    else if ("strokeWidth" in property) this.inner.setStrokeWidth(property.strokeWidth);
    else if ("strokeStyle" in property) this.inner.setStrokeStyle(property.strokeStyle);
    else if ("sloppiness" in property) this.inner.setSloppiness(property.sloppiness);
    else if ("edges" in property) this.inner.setEdges(property.edges);
    else if ("opacity" in property) this.inner.setOpacity(property.opacity);
    else if ("fontSize" in property) this.inner.setFontSize(property.fontSize);
    else this.inner.setTextAlign(property.textAlign);
  }

  /** The text view to show, `undefined` if no text is edited; read it with the viewport after every input or command. */
  textEditor(): TextEditor | undefined {
    const shown = this.inner.textEditor();
    if (!shown) return undefined;
    const result: TextEditor = {
      text: shown.text,
      x: shown.x,
      y: shown.y,
      width: shown.width,
      height: shown.height,
      fontSize: shown.fontSize,
      lineHeight: shown.lineHeight,
      fontFamily: shown.fontFamily,
      align: shown.align,
      color: shown.color,
      opacity: shown.opacity,
      angle: shown.angle,
      zoom: shown.zoom,
      fixedWidth: shown.fixedWidth,
    };
    shown.free();
    return result;
  }

  /** Tells the core what the text view says now. */
  setEditingText(text: string): void {
    this.inner.setEditingText(text);
  }

  /** Ends the editing and keeps the text (one undo step); empty text is not kept. */
  commitText(): void {
    this.inner.commitText();
  }

  /** Ends the editing and keeps the text as it was. */
  cancelText(): void {
    this.inner.cancelText();
  }

  /** Enter: edits the text or the Label of the selected Element. */
  editSelectedText(): void {
    this.inner.editSelectedText();
  }

  duplicate(): void {
    this.inner.duplicate();
  }

  group(): void {
    this.inner.group();
  }

  ungroup(): void {
    this.inner.ungroup();
  }

  selectAll(): void {
    this.inner.selectAll();
  }

  /**
   * A finger or mouse held at the view point `(x, y)`: which Context menu to
   * show (selecting what is pressed), `undefined` if the press means nothing
   * (the Pencil, another Tool, text being edited).
   */
  longPress(x: number, y: number, kind: PointerKind): ContextMenuKind | undefined {
    const menu = this.inner.longPress(x, y, kind);
    if (!menu) return undefined;
    const result: ContextMenuKind = menu.element
      ? { element: true, canGroup: menu.canGroup, canUngroup: menu.canUngroup }
      : { element: false };
    menu.free();
    return result;
  }

  deleteSelection(): void {
    this.inner.deleteSelection();
  }

  moveInZOrder(how: ZOrder): void {
    this.inner.moveInZOrder(how);
  }

  /** Lines up the Selection's groups and Elements; one undo step. */
  align(how: Alignment): void {
    this.inner.align(how);
  }

  /** Spaces the Selection's groups and Elements evenly; one undo step. */
  distribute(how: Distribution): void {
    this.inner.distribute(how);
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
