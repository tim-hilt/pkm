import { CanvasRenderer } from "./rendering/CanvasRenderer";
import { SceneSync } from "./rendering/SceneSync";
import { cssCursor } from "./rendering/CursorStyle";
import { CanvasInput } from "./input/CanvasInput";
import { ImageInput, pickImages } from "./images/ImageInput";
import { CanvasClipboard } from "./input/CanvasClipboard";
import { isCopyPngShortcut } from "./input/KeyboardShortcuts";
import { exportPng } from "./export/PngExport";
import { insertImages } from "./images/ImageInsertion";
import { ZoomControls } from "./chrome/ZoomControls";
import { Toolbar } from "./chrome/Toolbar";
import { UndoRedoControls } from "./chrome/UndoRedoControls";
import { PropertiesPanel } from "./chrome/PropertiesPanel";
import { PhoneChrome } from "./chrome/PhoneChrome";
import { ContextMenu } from "./chrome/ContextMenu";
import { type MenuEntry, openContextMenu } from "./chrome/ContextMenuModel";
import { MainMenu } from "./chrome/MainMenu";
import { TextEditorOverlay } from "./text/TextEditorOverlay";
import { caretClearOf, keyboardCovering } from "./text/KeyboardAvoidance";
import type { Appearance } from "../settings/Appearance";
import type { DrawingSettings } from "./chrome/DrawingSettings";
import type { DrawingExport } from "./export/DrawingExport";
import type { Editor } from "../core/Core";
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
  /** Whether the Canvas and chrome are dark, and when that changes. */
  appearance: Appearance;
}

/**
 * Puts a canvas that fills `root` and the chrome over it, draws `editor`'s
 * Scene on the canvas and forwards the input on it to the editor, as the Swift
 * `CanvasMetalView` does: after every input or command the Scene (if its
 * revision moved), the viewport, the cursor and the chrome are shown again.
 * Returns what stops it all, for when the Drawing is left, and `refresh`, to
 * show the Scene again after something besides input changed it (Sync).
 */
export function showCanvas(
  root: HTMLElement,
  device: GPUDevice,
  editor: Editor,
  font: Excalifont,
  handlers: CanvasHandlers,
): { stop: () => void; refresh: () => void } {
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("webgpu")!;
  const format = navigator.gpu.getPreferredCanvasFormat();
  context.configure({ device, format, alphaMode: "opaque" });

  const renderer = new CanvasRenderer(device, format, font);
  renderer.files = editor;
  const isDark = () => handlers.appearance.isDark;
  editor.setDarkMode(isDark());
  renderer.isDark = isDark();
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

  const textView = new TextEditorOverlay();
  textView.onChange = (text) => {
    editor.setEditingText(text);
    didReachEditor();
  };
  textView.onEnd = () => {
    editor.commitText();
    didReachEditor();
  };

  const sync = new SceneSync(editor, renderer);
  sync.showScene();

  let pointer: { x: number; y: number } | undefined;
  const zoomControls = new ZoomControls(editor, () => didReachEditor());
  const undoRedo = new UndoRedoControls(editor, () => didReachEditor());
  /** Images from the file picker, a drop or a paste; they are prepared asynchronously and inserted once ready. */
  const images = {
    images: (sources: Uint8Array[], at?: { x: number; y: number }) =>
      void insertImages(editor, sources, at).then((inserted) => {
        if (inserted && !gone) didReachEditor();
      }),
  };
  const clipboard = new CanvasClipboard(editor, navigator.clipboard, {
    ...images,
    changed: () => didReachEditor(),
    png: (scope) => exportPng(device, font, editor, scope, handlers.settings.exportBackground),
  });
  const insertFromPicker = () => void pickImages(images);
  const toolbar = new Toolbar(editor, () => didReachEditor(), insertFromPicker);
  const properties = new PropertiesPanel(editor, () => didReachEditor(), isDark);
  const phone = new PhoneChrome(editor, () => didReachEditor(), isDark, insertFromPicker);
  // The properties' chrome leaves the focus in the Text view, as the native
  // app's does: a property picked while typing styles the text being edited,
  // which stays open (the core's rule), where the Text view's blur would
  // commit it first. Fields still take the focus.
  const keepTextViewFocus = (e: MouseEvent) => {
    if (textView.isOpen && !(e.target instanceof HTMLInputElement)) e.preventDefault();
  };
  properties.element.addEventListener("mousedown", keepTextViewFocus);
  phone.element.addEventListener("mousedown", keepTextViewFocus);
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
  back.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M15 5l-7 7 7 7"/></svg>`;
  back.setAttribute("aria-label", "Back to Drawings");
  back.title = "Back to Drawings";
  back.addEventListener("click", () => handlers.back());
  const bottomLeft = document.createElement("div");
  bottomLeft.className = "bottom-left";
  bottomLeft.append(zoomControls.element, undoRedo.element);
  /**
   * Shows the Text view where the core says, or takes it away if no text is
   * edited; like the viewport it follows panning and zooming. Keeps the
   * caret's line above the on-screen keyboard, which only pans the viewport
   * (and does not pan back afterwards). Returns whether it panned.
   */
  const syncTextView = (): boolean => {
    const shown = editor.textEditor();
    if (!shown) {
      textView.hide();
      return false;
    }
    textView.show(shown, root);
    const visual = window.visualViewport;
    const caret = textView.caret(shown);
    if (!visual || !caret) return false;
    const bounds = canvas.getBoundingClientRect();
    const keyboard = keyboardCovering(bounds, visual, document.documentElement.clientHeight);
    const panned = caretClearOf(keyboard, caret, editor.viewport());
    if (!panned) return false;
    editor.setViewport(panned);
    return true;
  };
  /** Shows whatever the editor did with an input or command. */
  const didReachEditor = () => {
    // Panning for the keyboard moves the Text view too: shown once more.
    if (syncTextView()) syncTextView();
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
  const contextMenu = new ContextMenu();
  /** The entries, each showing what it did to the editor. */
  const wrapped = (entries: MenuEntry[]): MenuEntry[] =>
    entries.map((entry) => ({
      ...entry,
      children: entry.children && wrapped(entry.children),
      run:
        entry.run &&
        (() => {
          const done = entry.run!();
          didReachEditor();
          return done;
        }),
    }));
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
    shortcut: ({ key, modifiers }) => {
      if (isCopyPngShortcut({ key, modifiers })) {
        void clipboard.copyPng();
        return true;
      }
      const handled = editor.pressKey(key, modifiers);
      if (handled) didReachEditor();
      return handled;
    },
    contextMenu: (x, y, kind) => {
      // Selecting what is pressed is the core's: the chrome shows it.
      const entries = openContextMenu(editor, clipboard, { x, y }, kind);
      didReachEditor();
      if (!entries) return;
      const bounds = canvas.getBoundingClientRect();
      contextMenu.show(wrapped(entries), bounds.left + x, bounds.top + y);
    },
    hover: (x, y) => {
      pointer = { x, y };
      canvas.style.cursor = cssCursor(editor.cursor(x, y));
    },
  });

  const imageInput = new ImageInput(canvas, images);
  const stopClipboard = clipboard.listen(document, () => pointer);
  // Off the Canvas, a paste from the keyboard lands in the middle of the view.
  canvas.addEventListener("pointerleave", () => (pointer = undefined));

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
    contextMenu.element,
  );
  // The keyboard coming up or going shrinks or grows the visual viewport.
  const visual = window.visualViewport;
  const keyboardChanged = () => {
    if (syncTextView()) didReachEditor();
  };
  visual?.addEventListener("resize", keyboardChanged);
  // Dark mode is display only: the Scene, Selection, Text view and history stay
  // as they are; what is shown is made again in the new colours.
  const stopFollowingAppearance = handlers.appearance.subscribe(() => {
    editor.setDarkMode(isDark());
    renderer.isDark = isDark();
    sync.showScene();
    didReachEditor();
  });
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

  const stop = () => {
    stopFollowingAppearance();
    input.dispose();
    imageInput.dispose();
    stopClipboard();
    mainMenu.dispose();
    contextMenu.hide();
    visual?.removeEventListener("resize", keyboardChanged);
    textView.hide();
    resizing.disconnect();
    stopWatchingPixelRatio();
    gone = true;
    renderer.dispose();
    sync.dispose();
    context.unconfigure();
    root.replaceChildren();
  };
  return { stop, refresh: didReachEditor };
}
