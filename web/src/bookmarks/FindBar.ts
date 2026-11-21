import { button, make } from "../Dom";

/** What the Find bar asks of the Reader. */
export interface FindActions {
  /** The query changed. */
  query(text: string): void;
  /** Jump to the next match, or the previous one for -1. */
  step(direction: 1 | -1): void;
  close(): void;
}

/**
 * Article Search's bar at the bottom of the Reader: a field, which match is current, previous and
 * next, close. Enter Jumps on, Shift+Enter back, Escape closes.
 */
export class FindBar {
  readonly element = make("div", "reader-find");
  private readonly field = make("input", "tasks-input");
  private readonly count = make("span", "reader-find-count");

  constructor(actions: FindActions) {
    this.element.hidden = true;
    this.element.setAttribute("role", "search");
    this.field.type = "search";
    this.field.placeholder = "Find in Article";
    this.field.ariaLabel = "Find in Article";
    this.field.addEventListener("input", () => actions.query(this.field.value));
    this.field.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        actions.step(event.shiftKey ? -1 : 1);
      } else if (event.key === "Escape") {
        event.stopPropagation();
        actions.close();
      }
    });
    this.count.setAttribute("aria-live", "polite");
    const previous = button("tasks-back", "Previous", () => actions.step(-1));
    const next = button("tasks-back", "Next", () => actions.step(1));
    this.element.append(
      this.field,
      this.count,
      previous,
      next,
      button("tasks-back", "Done", () => actions.close()),
    );
  }

  get isOpen(): boolean {
    return !this.element.hidden;
  }

  /** Shows the bar with the field focused and its text selected. */
  open(): void {
    this.element.hidden = false;
    this.field.focus();
    this.field.select();
  }

  /** Hides the bar and clears its query. */
  close(): void {
    this.element.hidden = true;
    this.field.value = "";
    this.count.textContent = "";
  }

  /** Says which of `total` matches is `current` (counting from 0). */
  show(current: number, total: number): void {
    this.count.textContent =
      this.field.value === "" ? "" : total === 0 ? "No Matches" : `${current + 1} of ${total}`;
  }
}
