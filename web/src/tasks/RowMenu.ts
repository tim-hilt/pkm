import { button, make } from "../Dom";
import { HoldDetector } from "../drawings/input/HoldDetector";
import { menuCorner } from "../drawings/gallery/CardMenu";

/** One choice of a group, shown as a circle that only its `label` names for assistive technology. */
export interface RowMenuChoice {
  label: string;
  run: () => void;
  checked: boolean;
  /** A class that colours the circle (it takes the text colour). */
  className?: string;
}

/** One entry of a row's menu: a line, or a horizontal group of choices. */
export type RowMenuItem = RowMenuLine | { choices: RowMenuChoice[] };

export interface RowMenuLine {
  label: string;
  run: () => void;
  danger?: boolean;
  /** Shown as a choice among others, checked or not. */
  checked?: boolean;
  /** A heading shown above this entry, starting a section. */
  heading?: string;
  /** A divider shown above this entry, starting a section. */
  separated?: boolean;
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
    this.element.replaceChildren(...items.flatMap((item) => this.entries(item)));
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
    const first = this.element.querySelector<HTMLElement>(".row-menu-choices [aria-checked=true]");
    (first ?? this.element.querySelector<HTMLElement>("[role^=menuitem]"))?.focus();
  }

  /** The elements of `item`: its entry, with the heading or divider that starts its section. */
  private entries(item: RowMenuItem): HTMLElement[] {
    const choose = (run: () => void) => () => {
      this.hide();
      this.focusAfter?.focus();
      run();
    };
    const radio = (entry: HTMLElement, checked: boolean) => {
      entry.setAttribute("role", "menuitemradio");
      entry.setAttribute("aria-checked", String(checked));
    };
    if ("choices" in item) {
      const group = make("div", "row-menu-choices");
      group.setAttribute("role", "group");
      for (const { label, run, checked, className } of item.choices) {
        const circle = button(`row-menu-choice ${className ?? ""}`, "", choose(run));
        circle.setAttribute("aria-label", label);
        circle.title = label;
        radio(circle, checked);
        group.append(circle);
      }
      return [group];
    }
    const { label, run, danger, checked, heading, separated } = item;
    const entry = button(`row-menu-item${danger ? " danger" : ""}`, label, choose(run));
    if (checked === undefined) entry.setAttribute("role", "menuitem");
    else radio(entry, checked);
    const before: HTMLElement[] = [];
    if (separated) before.push(make("div", "row-menu-divider"));
    if (heading !== undefined) before.push(make("div", "row-menu-heading", [heading]));
    return [...before, entry];
  }

  private key(event: KeyboardEvent): void {
    const active = document.activeElement as HTMLElement;
    const group = active.closest<HTMLElement>(".row-menu-choices");
    if (event.key === "Escape") {
      this.hide();
      this.focusAfter?.focus();
    } else if (group && (event.key === "ArrowLeft" || event.key === "ArrowRight")) {
      const circles = [...group.children] as HTMLElement[];
      const step = event.key === "ArrowRight" ? 1 : -1;
      circles[(circles.indexOf(active) + step + circles.length) % circles.length].focus();
    } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      // A group is one stop, entered at its checked choice (else the first).
      const stops = [
        ...this.element.querySelectorAll<HTMLElement>(
          ":scope > :is(.row-menu-item, .row-menu-choices)",
        ),
      ];
      const at = stops.findIndex((stop) => stop.contains(active));
      const step = event.key === "ArrowDown" ? 1 : -1;
      const next = stops[(at + step + stops.length) % stops.length];
      const target = next.matches(".row-menu-choices")
        ? (next.querySelector<HTMLElement>("[aria-checked=true]") ?? next.firstElementChild)
        : next;
      (target as HTMLElement).focus();
    } else return;
    event.preventDefault();
  }
}
