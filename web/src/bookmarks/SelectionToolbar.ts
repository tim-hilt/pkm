import { button, make } from "../Dom";

/** What the toolbar offers on selected text. */
export interface SelectionActions {
  /** Highlights the selection in the Default Highlight Color. */
  highlight(): void;
  /** Puts a Comment on the spot where the selection ends. */
  comment(): void;
}

/**
 * The buttons that appear below selected text: Highlight and Comment. Pressing them keeps the
 * selection, which the actions read.
 */
export class SelectionToolbar {
  readonly element = make("div", "reader-selection");

  constructor(actions: SelectionActions) {
    this.element.hidden = true;
    this.element.setAttribute("role", "toolbar");
    this.element.setAttribute("aria-label", "Selection");
    this.element.append(
      button("tasks-back", "Highlight", () => actions.highlight()),
      button("tasks-back", "Comment", () => actions.comment()),
    );
    // Pressing a button must not clear the selection it acts on.
    this.element.addEventListener("pointerdown", (event) => event.preventDefault());
  }

  /** Shows it below `anchor`, within the viewport. */
  show(anchor: DOMRect): void {
    const { element } = this;
    element.hidden = false;
    const left = Math.max(8, Math.min(anchor.left, innerWidth - element.offsetWidth - 8));
    const top = Math.min(anchor.bottom + 8, innerHeight - element.offsetHeight - 8);
    element.style.left = `${left}px`;
    element.style.top = `${top}px`;
  }

  hide(): void {
    this.element.hidden = true;
  }
}
