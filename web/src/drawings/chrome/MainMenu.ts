import { ExportScope } from "../../core/Core";

/** What the main menu does and shows; `MainMenu.update` reads the state again after every event. */
export interface MainMenuActions {
  downloadDrawing(): void;
  downloadPng(scope: ExportScope): void;
  hasSelection(): boolean;
  exportBackground(): boolean;
  setExportBackground(on: boolean): void;
  pencilOnlyDrawing(): boolean;
  setPencilOnlyDrawing(on: boolean): void;
}

const moreGlyph = `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>`;

/**
 * Excalidraw's main menu, top right, a port of the Swift `MainMenu`: downloads
 * the Drawing as `.excalidraw`, exports PNG of the Scene or the Selection, and
 * the settings of PNG export (background) and Pen-only drawing. Native
 * elements with the menu roles, so that it is labelled and reachable by keyboard.
 */
export class MainMenu {
  readonly element = document.createElement("div");
  private readonly toggle = document.createElement("button");
  private readonly menu = document.createElement("div");
  private readonly selectionItem: HTMLButtonElement;
  private readonly background: HTMLButtonElement;
  private readonly penOnly: HTMLButtonElement;
  private stopWatching: (() => void) | undefined;

  constructor(private readonly actions: MainMenuActions) {
    this.element.className = "main-menu";
    this.toggle.type = "button";
    this.toggle.className = "main-menu-button";
    this.toggle.innerHTML = moreGlyph;
    this.toggle.title = "Menu";
    this.toggle.setAttribute("aria-label", "Menu");
    this.toggle.setAttribute("aria-haspopup", "menu");
    this.toggle.setAttribute("aria-expanded", "false");
    this.toggle.addEventListener("click", () => (this.menu.hidden ? this.show() : this.hide()));

    this.menu.className = "main-menu-items";
    this.menu.hidden = true;
    this.menu.setAttribute("role", "menu");
    this.menu.setAttribute("aria-label", "Main menu");
    this.menu.addEventListener("keydown", (event) => this.key(event));

    this.selectionItem = this.item("Export Selection as PNG", "menuitem", () =>
      actions.downloadPng(ExportScope.Selection),
    );
    this.background = this.item("PNG background", "menuitemcheckbox", () =>
      actions.setExportBackground(!actions.exportBackground()),
    );
    this.penOnly = this.item("Pen-only drawing", "menuitemcheckbox", () =>
      actions.setPencilOnlyDrawing(!actions.pencilOnlyDrawing()),
    );
    this.menu.append(
      this.item("Download Drawing", "menuitem", () => actions.downloadDrawing()),
      this.item("Export Scene as PNG", "menuitem", () => actions.downloadPng(ExportScope.Scene)),
      this.selectionItem,
      this.separator("Settings"),
      this.background,
      this.penOnly,
    );
    this.element.append(this.toggle, this.menu);
    this.update();
  }

  /** Shows the state of the Selection and the settings as they are now. */
  update(): void {
    this.selectionItem.disabled = !this.actions.hasSelection();
    this.background.setAttribute("aria-checked", String(this.actions.exportBackground()));
    this.penOnly.setAttribute("aria-checked", String(this.actions.pencilOnlyDrawing()));
  }

  dispose(): void {
    this.hide();
  }

  private item(label: string, role: string, run: () => void): HTMLButtonElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "main-menu-item";
    element.textContent = label;
    element.setAttribute("role", role);
    if (role === "menuitemcheckbox") element.setAttribute("aria-checked", "false");
    element.addEventListener("click", () => {
      // A setting stays open to be seen changing; an action closes the menu.
      if (role === "menuitem") {
        this.hide();
        this.toggle.focus();
      }
      run();
      this.update();
    });
    return element;
  }

  private separator(label: string): HTMLElement {
    const element = document.createElement("div");
    element.className = "main-menu-heading";
    element.setAttribute("role", "separator");
    element.textContent = label;
    return element;
  }

  private show(): void {
    this.update();
    this.menu.hidden = false;
    this.toggle.setAttribute("aria-expanded", "true");
    const away = (event: Event) => {
      if (!this.element.contains(event.target as Node)) this.hide();
    };
    document.addEventListener("pointerdown", away, true);
    this.stopWatching = () => document.removeEventListener("pointerdown", away, true);
    this.items()[0]?.focus();
  }

  private hide(): void {
    this.stopWatching?.();
    this.stopWatching = undefined;
    this.menu.hidden = true;
    this.toggle.setAttribute("aria-expanded", "false");
  }

  private items(): HTMLButtonElement[] {
    return [...this.menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled)")];
  }

  private key(event: KeyboardEvent): void {
    const items = this.items();
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    if (event.key === "Escape") {
      this.hide();
      this.toggle.focus();
    } else if (event.key === "ArrowDown") items[(at + 1) % items.length].focus();
    else if (event.key === "ArrowUp") items[(at - 1 + items.length) % items.length].focus();
    else return;
    event.preventDefault();
  }
}
