import type { Editor } from "../../core/Core";

/** Where settings are kept per Device: the browser's `localStorage`. */
export interface SettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const pencilOnlyKey = "pencilOnlyDrawing";
const dimensionsKey = "showDimensions";

/**
 * The settings of the main menu, kept per Device: Pencil-only drawing and
 * Show dimensions. A port of the Swift `CanvasController`'s handling of them.
 *
 * Show dimensions is only ever the user's choice, off until made. The core turns Pencil-only drawing on by itself with the first pen input.
 * It is stored once it is on or the user chose, and restored into each editor
 * before it is first read. Only a stored value is a decision; without one the
 * core decides.
 */
export class DrawingSettings {
  private restored: Editor | undefined;

  constructor(private readonly storage: SettingsStorage) {}

  /** Whether `editor` has Pencil-only drawing on, after restoring the stored choice into it (once per editor) and storing what the core decided. Call after every input. */
  pencilOnlyDrawing(editor: Editor): boolean {
    this.restore(editor);
    const stored = this.storage.getItem(pencilOnlyKey);
    const on = editor.pencilOnlyDrawing();
    if (on !== (stored === "true")) this.storage.setItem(pencilOnlyKey, String(on));
    return on;
  }

  /** The user's choice; stored even when off, so that a pen no longer turns it on by itself. */
  setPencilOnlyDrawing(editor: Editor, on: boolean): void {
    this.storage.setItem(pencilOnlyKey, String(on));
    editor.setPencilOnlyDrawing(on);
  }

  /** Whether `editor` shows Dimensions, after restoring the stored choice into it (once per editor). */
  showDimensions(editor: Editor): boolean {
    this.restore(editor);
    return editor.showDimensions();
  }

  setShowDimensions(editor: Editor, on: boolean): void {
    this.restore(editor);
    this.storage.setItem(dimensionsKey, String(on));
    editor.setShowDimensions(on);
  }

  /** Puts the stored choices into `editor`, the first time it is seen. */
  private restore(editor: Editor): void {
    if (this.restored === editor) return;
    this.restored = editor;
    const pencilOnly = this.storage.getItem(pencilOnlyKey);
    if (pencilOnly !== null) editor.setPencilOnlyDrawing(pencilOnly === "true");
    editor.setShowDimensions(this.storage.getItem(dimensionsKey) === "true");
  }
}
