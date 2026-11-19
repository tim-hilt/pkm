import { button, make } from "../Dom";
import type { RowMenu } from "../tasks/RowMenu";
import { deleteFolderDialog, moveDialog, nameDialog, reason } from "./FolderDialogs";
import { type Dragged, draggable, droppable } from "./FolderDrag";
import type { FolderRow, NotesModel } from "./NotesModel";

/**
 * The Folder outline: the top level and every Folder indented below its
 * parent, one selected. A Folder is made from the header's button or a row's
 * menu (right click, long press), which also renames, Moves and Deletes it;
 * Folders and Notes are dragged onto a row to be Moved there. A Move the core
 * refuses is told in a line below. A port of the Swift `FolderTree`.
 */
export class FolderOutline {
  readonly element = make("div", "notes-tree");
  private readonly list = make("ul", "tasks-list");
  private readonly refusal = make("p", "tasks-failure");

  constructor(
    private readonly model: NotesModel,
    private readonly menu: RowMenu,
    private readonly host: HTMLElement,
  ) {
    this.refusal.setAttribute("role", "alert");
    this.refusal.hidden = true;
    this.list.setAttribute("aria-label", "Folders");
    const create = button("tasks-back", "New Folder", () => this.create(model.folder));
    this.element.append(
      make("div", "notes-bar", [create]),
      make("div", "tasks-scroll", [this.list]),
      this.refusal,
    );
  }

  /** Shows the Folders as the model has them. */
  update(): void {
    this.list.replaceChildren(...this.model.outline.map((folder) => this.row(folder)));
  }

  /** Moves what was dropped or chosen into the Folder at `path`, telling why if it can't be. */
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

  private row({ name, path, depth }: FolderRow): HTMLElement {
    const open = button("task-open folder-open", name, () => this.model.select(path));
    open.style.paddingLeft = `${depth * 16 + 8}px`;
    if (path === this.model.folder) open.setAttribute("aria-current", "true");
    const item = make("li", "task-row", [open]);
    droppable(item, (what) => this.move(what, path));
    const isTop = path === "";
    if (!isTop) draggable(item, { folder: path });
    this.menu.attach(item, () => [
      { label: "New Folder", run: () => this.create(path) },
      ...(isTop
        ? []
        : [
            { label: "Rename", run: () => this.rename(path, name) },
            {
              label: "Move to…",
              run: () =>
                moveDialog(this.host, this.model, (into) => this.model.moveFolder(path, into)),
            },
            {
              label: "Delete",
              danger: true,
              run: () => deleteFolderDialog(this.host, this.model, path),
            },
          ]),
    ]);
    return item;
  }

  private create(parent: string): void {
    nameDialog(this.host, "New Folder", "Create", "", (name) => this.model.addFolder(name, parent));
  }

  private rename(path: string, name: string): void {
    nameDialog(this.host, "Rename Folder", "Rename", name, (to) =>
      this.model.renameFolder(path, to),
    );
  }
}
