/** What is dragged: a Note by its ID, or a Folder by its path. */
export type Dragged = { note: string } | { folder: string };

const NOTE = "application/x-pkm-note";
const FOLDER = "application/x-pkm-folder";

/** Lets `row` be dragged as `what`, to a Folder row or the top level. */
export function draggable(row: HTMLElement, what: Dragged): void {
  row.draggable = true;
  row.addEventListener("dragstart", (event) => {
    if (!event.dataTransfer) return;
    event.dataTransfer.effectAllowed = "move";
    if ("note" in what) event.dataTransfer.setData(NOTE, what.note);
    else event.dataTransfer.setData(FOLDER, what.folder);
  });
}

/** Lets a Note or Folder be dropped on `target`, which is marked with `drop-target` meanwhile; `drop` gets it, not a target around. */
export function droppable(target: HTMLElement, drop: (what: Dragged) => void): void {
  const accepts = (event: DragEvent) =>
    event.dataTransfer?.types.some((type) => type === NOTE || type === FOLDER) ?? false;
  const unmark = () => target.classList.remove("drop-target");
  target.addEventListener("dragover", (event) => {
    if (!accepts(event)) return;
    event.preventDefault();
    event.stopPropagation();
    target.classList.add("drop-target");
  });
  target.addEventListener("dragleave", unmark);
  target.addEventListener("drop", (event) => {
    unmark();
    const data = event.dataTransfer;
    if (!data || !accepts(event)) return;
    event.preventDefault();
    event.stopPropagation();
    const note = data.getData(NOTE);
    drop(note !== "" ? { note } : { folder: data.getData(FOLDER) });
  });
}
