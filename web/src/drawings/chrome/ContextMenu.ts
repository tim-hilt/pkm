import type { MenuEntry } from "./ContextMenuModel";

/**
 * The Context menu as DOM (ADR-0006), the web counterpart of the system's
 * edit menu: native buttons with the menu roles, so that it is labelled and
 * reachable by keyboard (arrows, Home/End, Enter, Escape). A submenu opens
 * in place below its entry. A press outside only closes it: that press goes
 * no further, so that it does not also change the Selection.
 */
export class ContextMenu {
  readonly element = document.createElement("div");
  /** Ends the watch for a press outside or the window losing focus, while open. */
  private watching: AbortController | undefined;

  constructor() {
    this.element.className = "context-menu";
    this.element.hidden = true;
    this.element.setAttribute("role", "menu");
    this.element.setAttribute("aria-label", "Context menu");
    // The menu takes the keys, none reaching the Canvas's shortcuts; an entry
    // has the focus only once moved to.
    this.element.tabIndex = -1;
    this.element.addEventListener("keydown", (e) => {
      this.key(e);
      e.stopPropagation();
    });
    // The browser's own menu never shows, on the menu either.
    this.element.addEventListener("contextmenu", (e) => e.preventDefault());
  }

  /** Shows `entries` with the top left corner at the client point `(x, y)`, kept inside the window. */
  show(entries: MenuEntry[], x: number, y: number): void {
    this.hide();
    this.element.replaceChildren(...entries.map((entry) => this.build(entry)));
    this.element.hidden = false;
    const { width, height } = this.element.getBoundingClientRect();
    const margin = 8;
    this.element.style.left = `${Math.max(margin, Math.min(x, innerWidth - width - margin))}px`;
    this.element.style.top = `${Math.max(margin, Math.min(y, innerHeight - height - margin))}px`;
    const away = (e: Event) => {
      if (this.element.contains(e.target as Node)) return;
      e.stopPropagation();
      e.preventDefault();
      this.hide();
    };
    this.watching = new AbortController();
    const { signal } = this.watching;
    document.addEventListener("pointerdown", away, { capture: true, signal });
    window.addEventListener("blur", () => this.hide(), { signal });
    this.element.focus();
  }

  hide(): void {
    this.watching?.abort();
    this.watching = undefined;
    this.element.hidden = true;
    this.element.replaceChildren();
  }

  private build(entry: MenuEntry): HTMLElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "context-menu-item";
    if (entry.destructive) button.classList.add("danger");
    button.textContent = entry.label;
    button.setAttribute("role", "menuitem");
    button.disabled = entry.disabled === true;
    const children = entry.children;
    if (!children) {
      button.addEventListener("click", () => {
        this.hide();
        void entry.run?.();
      });
      return button;
    }
    const sub = document.createElement("div");
    sub.className = "context-menu-sub";
    sub.hidden = true;
    sub.setAttribute("role", "menu");
    sub.setAttribute("aria-label", entry.label);
    sub.append(...children.map((child) => this.build(child)));
    button.setAttribute("aria-haspopup", "menu");
    button.setAttribute("aria-expanded", "false");
    const toggle = (open: boolean) => {
      sub.hidden = !open;
      button.setAttribute("aria-expanded", String(open));
    };
    button.addEventListener("click", () => toggle(Boolean(sub.hidden)));
    button.addEventListener("keydown", (e) => {
      if (e.key === "ArrowRight" && sub.hidden) {
        toggle(true);
        (sub.firstElementChild as HTMLElement).focus();
        e.stopPropagation();
      }
    });
    sub.addEventListener("keydown", (e) => {
      if (e.key !== "ArrowLeft" && e.key !== "Escape") return;
      toggle(false);
      button.focus();
      e.stopPropagation();
      e.preventDefault();
    });
    const group = document.createElement("div");
    group.append(button, sub);
    group.style.display = "contents";
    return group;
  }

  /** The entries that can be reached now, in order. */
  private items(): HTMLButtonElement[] {
    return [...this.element.querySelectorAll<HTMLButtonElement>("button")].filter(
      (b) => !b.disabled && !b.closest("[hidden]"),
    );
  }

  private key(e: KeyboardEvent): void {
    const items = this.items();
    const at = items.indexOf(document.activeElement as HTMLButtonElement);
    let next: HTMLButtonElement | undefined;
    if (e.key === "Escape") {
      this.hide();
    } else if (e.key === "ArrowDown") next = items[(at + 1) % items.length];
    else if (e.key === "ArrowUp") next = items[(at - 1 + items.length) % items.length];
    else if (e.key === "Home") next = items[0];
    else if (e.key === "End") next = items[items.length - 1];
    else if (e.key === "Tab") this.hide();
    else return;
    e.preventDefault();
    next?.focus();
  }
}
