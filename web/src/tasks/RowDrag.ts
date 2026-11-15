import type { ListedTask } from "./TasksModel";

/**
 * Mouse drag and drop of Task rows, which only reorders. Dropping on a row
 * moves the dragged Task before it; below the last row, to the end of the
 * list. Touch keeps its long press for the row's menu, so a row is draggable
 * only once the mouse is pressed on it. Rows and end area attached to one
 * instance reorder among themselves only; `move` does the reordering.
 */
export class RowDrag {
  private dragged: ListedTask | undefined;
  private marked: HTMLElement | undefined;

  constructor(private readonly move: (task: ListedTask, before: ListedTask | undefined) => void) {}

  /** Makes `row`, which shows `task`, draggable and a drop target. */
  attachRow(row: HTMLElement, task: ListedTask): void {
    row.addEventListener("pointerdown", (event) => (row.draggable = event.pointerType === "mouse"));
    row.addEventListener("dragstart", (event) => {
      this.dragged = task;
      event.dataTransfer?.setData("text/plain", task.id);
      if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
    });
    row.addEventListener("dragend", () => this.end());
    row.addEventListener("dragover", (event) => {
      if (!this.dragged || this.dragged.id === task.id) return;
      event.preventDefault();
      event.stopPropagation();
      this.mark(row);
    });
    row.addEventListener("drop", (event) => {
      const moved = this.dragged;
      if (!moved || moved.id === task.id) return;
      event.preventDefault();
      event.stopPropagation();
      this.end();
      this.move(moved, task);
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

  private mark(row: HTMLElement | undefined): void {
    this.marked?.classList.remove("drop-before");
    this.marked = row;
    row?.classList.add("drop-before");
  }

  private end(): void {
    this.mark(undefined);
    this.dragged = undefined;
  }
}
