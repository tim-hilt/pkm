import { button } from "../Dom";
import { HoldDetector } from "../drawings/input/HoldDetector";
import { menuCorner } from "../drawings/gallery/CardMenu";

/** One entry of a row's menu. */
export interface RowMenuItem {
  label: string;
  run: () => void;
  danger?: boolean;
  /** Shown as a choice among others, checked or not. */
  checked?: boolean;
  /** A heading shown above this entry, starting a section. */
  heading?: string;
}

/**
 * The menu of a list's rows: one popup, which opens on the row that was right
 * clicked or long pressed (Safari on iOS sends no `contextmenu` for a long
 * press, so it is timed) with its corner at the press, kept within `bounds`.
 * Escape, a press elsewhere or choosing an entry closes it.
 */
export class RowMenu {
  readonly element = document.createElement("div");
  private closeOnPressElsewhere: (() => void) | undefined;
  /** Set when a long press opened the menu, so that the touch's release does not also open the row. */
  private swallowClick = false;
  private focusAfter: HTMLElement | undefined;

  constructor(private readonly bounds: HTMLElement) {
    this.element.className = "row-menu";
    this.element.setAttribute("role", "menu");
    this.element.hidden = true;
    this.element.addEventListener("keydown", (event) => this.key(event));
  }

  /**
   * Opens `items(pressed)` on `row` for right click and long press, `pressed`
   * being the element in it that was pressed; where they are `undefined`, the
   * press is left to the browser. `focusAfter` is where focus goes back to.
   */
  attach(
    row: HTMLElement,
    items: (pressed: Element) => RowMenuItem[] | undefined,
    focusAfter: HTMLElement = row,
  ): void {
    let pressed: Element = row;
    const hold = new HoldDetector((x, y) => {
      const chosen = items(pressed);
      if (!chosen) return;
      this.swallowClick = true;
      this.show(chosen, x, y, focusAfter);
    });
    row.addEventListener("contextmenu", (event) => {
      const chosen = items(event.target as Element);
      if (!chosen) return;
      event.preventDefault();
      this.show(chosen, event.clientX, event.clientY, focusAfter);
    });
    row.addEventListener("pointerdown", (event) => {
      this.swallowClick = false;
      pressed = event.target as Element;
      if (event.pointerType === "touch") hold.down(event.pointerId, event.clientX, event.clientY);
    });
    row.addEventListener("pointermove", (event) =>
      hold.move(event.pointerId, event.clientX, event.clientY),
    );
    for (const type of ["pointerup", "pointercancel"] as const) {
      row.addEventListener(type, (event) => hold.up(event.pointerId));
    }
    row.addEventListener(
      "click",
      (event) => {
        if (!this.swallowClick) return;
        this.swallowClick = false;
        event.stopPropagation();
      },
      true,
    );
  }

  hide(): void {
    this.closeOnPressElsewhere?.();
    this.closeOnPressElsewhere = undefined;
    this.element.hidden = true;
  }

  /** Opens `items` with its corner at (`x`, `y`), as a press on a row does. */
  show(items: RowMenuItem[], x: number, y: number, focusAfter: HTMLElement): void {
    this.hide();
    this.focusAfter = focusAfter;
    if (items.length === 0) return;
    this.element.replaceChildren(
      ...items.flatMap(({ label, run, danger, checked, heading }) => {
        const entry = button(`row-menu-item${danger ? " danger" : ""}`, label, () => {
          this.hide();
          focusAfter.focus();
          run();
        });
        if (checked === undefined) entry.setAttribute("role", "menuitem");
        else {
          entry.setAttribute("role", "menuitemradio");
          entry.setAttribute("aria-checked", String(checked));
        }
        if (heading === undefined) return [entry];
        const title = document.createElement("div");
        title.className = "row-menu-heading";
        title.textContent = heading;
        return [title, entry];
      }),
    );
    this.element.hidden = false;
    const size = { width: this.element.offsetWidth, height: this.element.offsetHeight };
    const { left, top } = menuCorner({ x, y }, this.bounds.getBoundingClientRect(), size);
    this.element.style.left = `${left}px`;
    this.element.style.top = `${top}px`;
    const away = (event: Event) => {
      if (!this.element.contains(event.target as Node)) this.hide();
    };
    document.addEventListener("pointerdown", away, true);
    this.closeOnPressElsewhere = () => document.removeEventListener("pointerdown", away, true);
    this.element.querySelector<HTMLElement>("[role^=menuitem]")?.focus();
  }

  private key(event: KeyboardEvent): void {
    const entries = [...this.element.querySelectorAll<HTMLElement>("[role^=menuitem]")];
    const at = entries.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      this.hide();
      this.focusAfter?.focus();
    } else if (event.key === "ArrowDown") entries[(at + 1) % entries.length].focus();
    else if (event.key === "ArrowUp") entries[(at - 1 + entries.length) % entries.length].focus();
    else return;
    event.preventDefault();
  }
}
