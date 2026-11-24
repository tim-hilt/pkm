/**
 * Mouse drag and drop of rows of Tasks or Filters (anything with an `id`).
 * Dropping on a row moves the dragged item before it; below the last row, to
 * the end of the list. Given `onto`, a row also takes drops onto it: its top
 * quarter moves before it, its middle half drops onto it, its bottom quarter
 * moves after it. Touch keeps its long press for the row's menu, so a row is
 * draggable only once the mouse is pressed on it. Rows and end area attached
 * to one instance reorder among themselves only: a row is never part of an end
 * area, so a drop on the dragged row or on another instance's row does nothing.
 */
export class RowDrag<T extends { id: string }> {
  private dragged: T | undefined;
  private marked: { row: HTMLElement; mark: Mark } | undefined;
  /** The rows attached, for finding the one after a row. */
  private readonly items = new WeakMap<Element, T>();

  constructor(
    private readonly move: (item: T, before: T | undefined) => void,
    private readonly onto?: (item: T, target: T) => void,
  ) {}

  /** Makes `row`, which shows `item`, draggable and a drop target. */
  attachRow(row: HTMLElement, item: T): void {
    this.items.set(row, item);
    row.addEventListener("pointerdown", (event) => (row.draggable = event.pointerType === "mouse"));
    row.addEventListener("dragstart", (event) => {
      this.dragged = item;
      event.dataTransfer?.setData("text/plain", item.id);
      if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
    });
    row.addEventListener("dragend", () => this.end());
    row.addEventListener("dragover", (event) => {
      event.stopPropagation();
      if (!this.dragged || this.dragged.id === item.id) return;
      event.preventDefault();
      this.mark(row, this.markAt(row, event));
    });
    row.addEventListener("drop", (event) => {
      const moved = this.dragged;
      if (!moved || moved.id === item.id) return;
      event.preventDefault();
      event.stopPropagation();
      const mark = this.markAt(row, event);
      this.end();
      if (mark === "onto") this.onto?.(moved, item);
      else if (mark === "before") this.move(moved, item);
      else if (this.after(row)?.id !== moved.id) this.move(moved, this.after(row));
    });
  }

  /** Makes `area`, the space below the rows, the drop target for the end of the list. */
  attachEnd(area: HTMLElement): void {
    area.addEventListener("dragover", (event) => {
      if (!this.dragged) return;
      event.preventDefault();
      this.mark(undefined);
    });
    area.addEventListener("drop", (event) => {
      const moved = this.dragged;
      if (!moved) return;
      event.preventDefault();
      this.end();
      this.move(moved, undefined);
    });
  }

  /** Where over `row` the pointer of `event` drops. */
  private markAt(row: HTMLElement, event: DragEvent): Mark {
    if (!this.onto) return "before";
    const box = row.getBoundingClientRect();
    const at = (event.clientY - box.top) / box.height;
    return at < 0.25 ? "before" : at > 0.75 ? "after" : "onto";
  }

  /** The item of the row after `row`, if it is one of this instance's. */
  private after(row: HTMLElement): T | undefined {
    return row.nextElementSibling ? this.items.get(row.nextElementSibling) : undefined;
  }

  private mark(row: HTMLElement | undefined, mark: Mark = "before"): void {
    if (this.marked) this.marked.row.classList.remove(`drop-${this.marked.mark}`);
    this.marked = row && { row, mark };
    row?.classList.add(`drop-${mark}`);
  }

  private end(): void {
    this.mark(undefined);
    this.dragged = undefined;
  }
}

type Mark = "before" | "onto" | "after";
