import { button, make, onNewKey } from "../Dom";
import { DescriptionField } from "../tasks/DescriptionField";
import { RowMenu } from "../tasks/RowMenu";
import { confirmButton, modal } from "../tasks/TaskDialogs";
import { titleNodes } from "../tasks/TitleText";
import { type ListedNote, type NotesModel } from "./NotesModel";
import { FolderOutline } from "./FolderOutline";
import { draggable } from "./FolderDrag";
import { moveDialog } from "./FolderDialogs";
import { TitleField } from "./TitleField";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Notes Context: the Folder outline, the selected Folder's Notes, newest
 * changed first, and the open Note's editor, side by side; where the window is
 * narrow one over the other (the CSS decides, from `has-folder` and
 * `has-open`). A Note is Moved by dragging it onto a Folder or with "Move to…"
 * in its row's menu, and Deleted from its row's menu (right
 * click, long press), with ⌫ on a row or from the editor, asking first when
 * its Body has text. A port of the Swift `NotesView`.
 */
export class NotesView {
  readonly element = make("main", "notes");
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty", [
    make("strong", "", ["No Notes"]),
    make("span", "", ["Tap + to write one."]),
  ]);
  private readonly detail = make("div", "notes-detail");
  private readonly failure = make("p", "tasks-failure");
  private readonly menu: RowMenu;
  private readonly outline: FolderOutline;
  private readonly folderName = make("span", "notes-folder");
  /** The Note the editor shows; the editor is built again for another. */
  private shown: string | undefined;

  constructor(private readonly model: NotesModel) {
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Notes");
    this.menu = new RowMenu(this.element);
    this.outline = new FolderOutline(model, this.menu, this.element);
    const create = button("tasks-new", "", () => model.add());
    create.innerHTML = plusGlyph;
    create.title = create.ariaLabel = "New Note";
    const side = make("div", "notes-side", [
      make("div", "notes-bar notes-nav", [
        button("tasks-back", "Folders", () => model.leaveFolder()),
        this.folderName,
      ]),
      make("div", "tasks-scroll", [this.list, this.empty]),
      this.failure,
    ]);
    this.element.append(this.outline.element, side, this.detail, create, this.menu.element);
    onNewKey(this.element, () => model.add());
    model.onChange = () => this.update();
    this.update();
  }

  /** Shows the list and the open Note as the model has them. */
  update(): void {
    const { model } = this;
    this.menu.hide();
    this.element.classList.toggle("has-folder", model.showsFolder);
    this.element.classList.toggle("has-open", model.openId !== undefined);
    this.outline.update();
    this.folderName.textContent = model.folder.slice(model.folder.lastIndexOf("/") + 1) || "Notes";
    this.list.replaceChildren(...model.rows.map((row) => this.row(row)));
    this.empty.hidden = model.rows.length > 0;
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
  }

  private row(row: ListedNote): HTMLElement {
    const open = button("task-open note-open", "", () => this.model.open(row.id));
    open.append(...this.rowText(row));
    const item = make("li", "task-row note-row", [open]);
    if (row.id === this.model.openId) open.setAttribute("aria-current", "true");
    draggable(item, { note: row.id });
    this.menu.attach(item, () => [
      {
        label: "Move to…",
        run: () => moveDialog(this.element, this.model, (to) => this.model.moveNote(row.id, to)),
      },
      { label: "Delete", danger: true, run: () => this.ask(row.id) },
    ]);
    item.addEventListener("keydown", (event) => {
      if (event.key === "Backspace" || event.key === "Delete") this.ask(row.id);
    });
    return item;
  }

  /** The Title formatted, or the Body's first line, or "New Note". */
  private rowText({ title, preview }: ListedNote): HTMLElement[] {
    const line = (text: string, className: string) => make("span", className, titleNodes(text));
    if (title !== "") {
      return [
        line(title, "task-title"),
        ...(preview !== "" ? [line(preview, "task-description")] : []),
      ];
    }
    return [preview !== "" ? line(preview, "task-title") : make("span", "note-none", ["New Note"])];
  }

  /** The editor of the open Note; built again only when another Note opens. */
  private showEditor(): void {
    const id = this.model.openId;
    if (id === this.shown) return;
    this.shown = id;
    const note = id === undefined ? undefined : this.model.note(id);
    if (id === undefined || !note) {
      this.detail.replaceChildren(
        make("div", "tasks-empty", [make("strong", "", ["No Note Selected"])]),
      );
      return;
    }
    const save = () => this.model.save(id, title.value, body.value);
    const body = new DescriptionField(note.body);
    body.element.setAttribute("aria-label", "Note");
    body.element.dataset.placeholder = "Note";
    const title = new TitleField(note.title, { change: save, submit: () => body.focus() });
    body.element.addEventListener("input", save);
    const bar = make("div", "notes-bar", [
      button("tasks-back", "Notes", () => this.model.open(undefined)),
      button("tasks-back", "Delete", () => this.ask(id)),
    ]);
    this.detail.replaceChildren(bar, make("div", "notes-editor", [title.element, body.element]));
    if (this.model.newId === id) title.focus();
  }

  /** Deletes the Note, or asks first if its Body has text. */
  private ask(id: string): void {
    const { model } = this;
    if (!model.confirmsDelete(id)) return model.delete(id);
    const confirm = confirmButton("Delete");
    const { dialog, form } = modal(
      this.element,
      "Delete this Note?",
      [],
      [button("", "Cancel", () => dialog.close()), confirm],
    );
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      model.delete(id);
      dialog.close();
    });
  }
}
