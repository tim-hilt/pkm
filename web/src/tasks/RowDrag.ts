import type { ListedTask, TasksModel } from "./TasksModel";

/** Where on a row a drop lands: its top third puts the Task before it, the rest under it. */
const beforeShare = 0.33;

/**
 * Mouse drag and drop of Task rows. Dropping on the top of a row moves the
 * dragged Task before it, as its sibling; on the rest of a row, under it as
 * its last Subtask; below the last row, to the end of the top level. Touch
 * keeps its long press for the row's menu, so a row is draggable only once
 * the mouse is pressed on it.
 */
export class RowDrag {
  private dragged: ListedTask | undefined;
  private marked: HTMLElement | undefined;

  constructor(private readonly model: TasksModel) {}

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
      this.mark(row, this.isBefore(event, row) ? "drop-before" : "drop-under");
    });
    row.addEventListener("drop", (event) => {
      const moved = this.dragged;
      if (!moved || moved.id === task.id) return;
      event.preventDefault();
      event.stopPropagation();
      const before = this.isBefore(event, row);
      this.end();
      if (before) this.model.moveBefore(moved, task);
      else this.model.moveUnder(moved, task);
    });
  }

  /** Makes `area`, the space below the rows, the drop target for the end of the top level. */
  attachEnd(area: HTMLElement): void {
    area.addEventListener("dragover", (event) => {
      if (!this.dragged) return;
      event.preventDefault();
      this.mark(undefined, "");
    });
    area.addEventListener("drop", (event) => {
      const moved = this.dragged;
      if (!moved) return;
      event.preventDefault();
      this.end();
      this.model.moveBefore(moved, undefined);
    });
  }

  private isBefore(event: DragEvent, row: HTMLElement): boolean {
    const { top, height } = row.getBoundingClientRect();
    return event.clientY - top < height * beforeShare;
  }

  private mark(row: HTMLElement | undefined, className: string): void {
    this.marked?.classList.remove("drop-before", "drop-under");
    this.marked = row;
    if (className) row?.classList.add(className);
  }

  private end(): void {
    this.mark(undefined, "");
    this.dragged = undefined;
  }
}
