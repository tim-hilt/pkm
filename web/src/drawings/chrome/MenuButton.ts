/**
 * A button that opens a menu under it, for the main menu and the phone's
 * zoom menu. Native elements with the menu roles, so that it is labelled and
 * reachable by keyboard. `onChange` runs after an item ran.
 */
export class MenuButton {
  readonly element = document.createElement("div");
  readonly toggle = document.createElement("button");
  private readonly menu = document.createElement("div");
  private stopWatching: (() => void) | undefined;

  constructor(
    label: string,
    private readonly onChange: () => void,
  ) {
    this.element.className = "menu";
    this.toggle.type = "button";
    this.toggle.className = "menu-button";
    this.toggle.title = label;
    this.toggle.setAttribute("aria-label", label);
    this.toggle.setAttribute("aria-haspopup", "menu");
    this.toggle.setAttribute("aria-expanded", "false");
    this.toggle.addEventListener("click", () => (this.menu.hidden ? this.show() : this.hide()));

    this.menu.className = "menu-items";
    this.menu.hidden = true;
    this.menu.setAttribute("role", "menu");
    this.menu.setAttribute("aria-label", label);
    this.menu.addEventListener("keydown", (event) => this.key(event));
    this.element.append(this.toggle, this.menu);
  }

  /** Fills the menu with `items`. */
  set(...items: HTMLElement[]): void {
    this.menu.replaceChildren(...items);
  }

  /** An item; an action (`menuitem`) closes the menu, a setting stays open to be seen changing. */
  item(
    label: string,
    role: "menuitem" | "menuitemcheckbox",
    run: () => void,
    shortcut?: { label: string; aria: string },
  ): HTMLButtonElement {
    const element = document.createElement("button");
    element.type = "button";
    element.className = "menu-item";
    element.textContent = label;
    element.setAttribute("role", role);
    if (shortcut) {
      const keys = document.createElement("kbd");
      keys.textContent = shortcut.label;
      keys.setAttribute("aria-hidden", "true");
      element.append(keys);
      element.setAttribute("aria-keyshortcuts", shortcut.aria);
    }
    if (role === "menuitemcheckbox") element.setAttribute("aria-checked", "false");
    element.addEventListener("click", () => {
      if (role === "menuitem") {
        this.hide();
        this.toggle.focus();
      }
      run();
      this.onChange();
    });
    return element;
  }

  separator(label: string): HTMLElement {
    const element = document.createElement("div");
    element.className = "menu-heading";
    element.setAttribute("role", "separator");
    element.textContent = label;
    return element;
  }

  hide(): void {
    this.stopWatching?.();
    this.stopWatching = undefined;
    this.menu.hidden = true;
    this.toggle.setAttribute("aria-expanded", "false");
  }

  private show(): void {
    this.menu.hidden = false;
    this.toggle.setAttribute("aria-expanded", "true");
    const away = (event: Event) => {
      if (!this.element.contains(event.target as Node)) this.hide();
    };
    document.addEventListener("pointerdown", away, true);
    this.stopWatching = () => document.removeEventListener("pointerdown", away, true);
    this.items()[0]?.focus();
  }

  private items(): HTMLButtonElement[] {
    return [
      ...this.menu.querySelectorAll<HTMLButtonElement>("button:not(:disabled):not([hidden])"),
    ];
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
