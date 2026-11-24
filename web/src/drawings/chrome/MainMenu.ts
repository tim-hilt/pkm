import { FileCommand, fileShortcutLabel, isApplePlatform } from "../input/KeyboardShortcuts";
import { MenuButton } from "./MenuButton";

/** What the main menu does and shows; `MainMenu.update` reads the state again after every event. */
export interface MainMenuActions {
  saveAs(): void;
  exportImage(): void;
  /** Whether there is something to export. */
  canExport(): boolean;
  pencilOnlyDrawing(): boolean;
  setPencilOnlyDrawing(on: boolean): void;
}

const moreGlyph = `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>`;

/**
 * Excalidraw's main menu, top right, a port of the Swift `MainMenu`: Save as
 * (the `.excalidraw` file), Export (the image) with their shortcuts, and the
 * Pen-only drawing setting.
 */
export class MainMenu {
  private readonly menu = new MenuButton("Menu", () => this.update());
  readonly element = this.menu.element;
  private readonly exportItem: HTMLButtonElement;
  private readonly penOnly: HTMLButtonElement;

  constructor(private readonly actions: MainMenuActions) {
    this.menu.toggle.innerHTML = moreGlyph;
    const apple = isApplePlatform(navigator.platform);
    this.exportItem = this.menu.item("Export…", "menuitem", () => actions.exportImage(), {
      label: fileShortcutLabel(FileCommand.Export, apple),
      aria: apple ? "Meta+Shift+E" : "Control+Shift+E",
    });
    this.penOnly = this.menu.item("Pen-only drawing", "menuitemcheckbox", () =>
      actions.setPencilOnlyDrawing(!actions.pencilOnlyDrawing()),
    );
    this.menu.set(
      this.menu.item("Save as…", "menuitem", () => actions.saveAs(), {
        label: fileShortcutLabel(FileCommand.SaveAs, apple),
        aria: apple ? "Meta+S" : "Control+S",
      }),
      this.exportItem,
      this.menu.separator("Settings"),
      this.penOnly,
    );
    this.update();
  }

  /** Shows the state of the Selection and the settings as they are now. */
  update(): void {
    this.exportItem.disabled = !this.actions.canExport();
    this.penOnly.setAttribute("aria-checked", String(this.actions.pencilOnlyDrawing()));
  }

  dispose(): void {
    this.menu.hide();
  }
}
