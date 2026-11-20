import { button, make } from "../Dom";
import type { RowMenu } from "../tasks/RowMenu";
import { deleteFolderDialog, moveDialog, nameDialog, reason } from "./FolderDialogs";
import { type Dragged, draggable, droppable } from "./FolderDrag";
import type { NotesModel } from "./NotesModel";

/**
 * The Folders' rows of the tree: a click selects one and opens or closes it,
 * its menu (right click, long press) makes a Folder in it, renames, Moves and
 * Deletes it. Folders and Notes are dragged onto a row, or a Note's row for
 * its Folder, to be Moved there. A Move the core refuses is told in `refusal`.
 * A port of the Swift `FolderContents`.
 */
export class FolderRows {
  readonly refusal = make("p", "tasks-failure");

  constructor(
    private readonly model: NotesModel,
    private readonly menu: RowMenu,
    private readonly host: HTMLElement,
  ) {
    this.refusal.setAttribute("role", "alert");
    this.refusal.hidden = true;
  }

  /** The row of the Folder at `path`, indented `depth` levels. */
  row(name: string, path: string, depth: number, expanded: boolean): HTMLElement {
    const open = button("task-open folder-open", name, () => this.model.toggle(path));
    open.style.paddingLeft = indent(depth);
    open.setAttribute("aria-expanded", String(expanded));
    if (path === this.model.folder && this.model.openId === undefined) {
      open.setAttribute("aria-current", "true");
    }
    const item = make("li", "task-row", [open]);
    this.dropsInto(item, path);
    draggable(item, { folder: path });
    this.menu.attach(item, () => [
      { label: "New Folder", run: () => this.create(path) },
      { label: "Rename", run: () => this.rename(path, name) },
      {
        label: "Move to…",
        run: () => moveDialog(this.host, this.model, (into) => this.model.moveFolder(path, into)),
      },
      { label: "Delete", danger: true, run: () => deleteFolderDialog(this.host, this.model, path) },
    ]);
    return item;
  }

  /** Lets what is dropped on `target` be Moved into the Folder at `path`. */
  dropsInto(target: HTMLElement, path: string): void {
    droppable(target, (what) => this.move(what, path));
  }

  /** Asks for the name of a new Folder in the one at `parent`. */
  create(parent: string): void {
    nameDialog(this.host, "New Folder", "Create", "", (name) => this.model.addFolder(name, parent));
  }

  /** Moves what was dropped into the Folder at `path`, telling why if it can't be. */
  private move(what: Dragged, path: string): void {
    this.refusal.hidden = true;
    try {
      if ("note" in what) this.model.moveNote(what.note, path);
      else this.model.moveFolder(what.folder, path);
    } catch (error) {
      this.refusal.textContent = reason(error);
      this.refusal.hidden = false;
    }
  }

  private rename(path: string, name: string): void {
    nameDialog(this.host, "Rename Folder", "Rename", name, (to) =>
      this.model.renameFolder(path, to),
    );
  }
}

/** The left padding of a row `depth` Folders deep. */
export function indent(depth: number): string {
  return `${depth * 16 + 8}px`;
}
