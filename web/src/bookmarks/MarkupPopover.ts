import { button, make } from "../Dom";

/** What the popover edits: a Highlight (with colors to choose from) or a Comment on a spot. */
export interface MarkupTarget {
  /** The Highlight's color and those to recolor it in, `0xRRGGBB`; none for a Comment on a spot. */
  color?: { current: number; choices: { id: string; rgb: number }[]; set(rgb: number): void };
  comment: string;
  /** An empty `comment` deletes the Comment, and a Comment on a spot with it. */
  setComment(comment: string): void;
  remove(): void;
}

const hex = (rgb: number) => `#${rgb.toString(16).padStart(6, "0")}`;

/**
 * The popover on a Highlight or a Comment: the Highlight Colors to recolor with, the Comment to
 * write, Delete. The Comment is kept when the popover closes (Escape, a press elsewhere, Delete).
 */
export class MarkupPopover {
  readonly element = make("div", "reader-popover");
  private close: (() => void) | undefined;

  constructor() {
    this.element.hidden = true;
    this.element.addEventListener("keydown", (event) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      this.close?.();
    });
  }

  /** Opens it below `anchor`, in the viewport, over what it was open on. */
  open(anchor: DOMRect, target: MarkupTarget): void {
    this.close?.();
    const comment = make("textarea", "tasks-input reader-comment");
    comment.ariaLabel = "Comment";
    comment.placeholder = "Comment";
    comment.rows = 3;
    comment.value = target.comment;
    const parts: HTMLElement[] = [];
    if (target.color) {
      const { current, choices, set } = target.color;
      const swatches = make("div", "reader-swatches");
      for (const { id, rgb } of choices) {
        const swatch = button("reader-swatch", "", () => {
          set(rgb);
          for (const other of swatches.children) {
            other.ariaPressed = String(other === swatch);
          }
        });
        swatch.style.backgroundColor = hex(rgb);
        swatch.ariaLabel = id;
        swatch.ariaPressed = String(rgb === current);
        swatches.append(swatch);
      }
      parts.push(swatches);
    }
    let removed = false;
    const finish = () => {
      document.removeEventListener("pointerdown", outside, true);
      this.element.hidden = true;
      this.element.replaceChildren();
      this.close = undefined;
      if (!removed && comment.value !== target.comment) target.setComment(comment.value);
    };
    const outside = (event: PointerEvent) => {
      if (!this.element.contains(event.target as Node)) finish();
    };
    parts.push(
      comment,
      button("tasks-back danger", "Delete", () => {
        removed = true;
        finish();
        target.remove();
      }),
    );
    this.element.replaceChildren(...parts);
    this.element.hidden = false;
    this.place(anchor);
    document.addEventListener("pointerdown", outside, true);
    this.close = finish;
    comment.focus();
  }

  dismiss(): void {
    this.close?.();
  }

  /** Below `anchor`, or above it where the viewport ends; kept within the viewport sideways. */
  private place(anchor: DOMRect): void {
    const { element } = this;
    const { offsetWidth: width, offsetHeight: height } = element;
    const left = Math.max(8, Math.min(anchor.left, innerWidth - width - 8));
    const below = anchor.bottom + 8;
    const top = below + height > innerHeight ? Math.max(8, anchor.top - height - 8) : below;
    element.style.left = `${left}px`;
    element.style.top = `${top}px`;
  }
}
