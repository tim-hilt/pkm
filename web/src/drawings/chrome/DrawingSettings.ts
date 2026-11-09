import type { Editor } from "../../core/Core";

/** Where settings are kept per Device: the browser's `localStorage`. */
export interface SettingsStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

const pencilOnlyKey = "pencilOnlyDrawing";
const exportBackgroundKey = "exportBackground";

/**
 * The settings of the main menu, kept per Device: whether a PNG export has
 * the white paper behind it (on until turned off) and Pencil-only drawing.
 * A port of the Swift `CanvasController`'s handling of the latter.
 *
 * The core turns Pencil-only drawing on by itself with the first pen input.
 * It is stored once it is on or the user chose, and restored into each editor
 * before it is first read. Only a stored value is a decision; without one the
 * core decides.
 */
export class DrawingSettings {
  private restored: Editor | undefined;

  constructor(private readonly storage: SettingsStorage) {}

  get exportBackground(): boolean {
    return this.storage.getItem(exportBackgroundKey) !== "false";
  }

  set exportBackground(on: boolean) {
    this.storage.setItem(exportBackgroundKey, String(on));
  }

  /** Whether `editor` has Pencil-only drawing on, after restoring the stored choice into it (once per editor) and storing what the core decided. Call after every input. */
  pencilOnlyDrawing(editor: Editor): boolean {
    const stored = this.storage.getItem(pencilOnlyKey);
    if (this.restored !== editor) {
      this.restored = editor;
      if (stored !== null) editor.setPencilOnlyDrawing(stored === "true");
    }
    const on = editor.pencilOnlyDrawing();
    if (on !== (stored === "true")) this.storage.setItem(pencilOnlyKey, String(on));
    return on;
  }

  /** The user's choice; stored even when off, so that a pen no longer turns it on by itself. */
  setPencilOnlyDrawing(editor: Editor, on: boolean): void {
    this.storage.setItem(pencilOnlyKey, String(on));
    editor.setPencilOnlyDrawing(on);
  }
}
