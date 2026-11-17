import { Closing, Planning } from "../core/Core";
import { button, make, onNewKey } from "../Dom";
import { filterDialog, deleteFilterDialog } from "./FilterDialogs";
import { filterRows } from "./FiltersList";
import { RowDrag } from "./RowDrag";
import { RowMenu, type RowMenuItem } from "./RowMenu";
import { taskLine } from "./TaskRow";
import { type TaskPlaces, editTaskDialog, newTaskDialog } from "./TaskDialogs";
import {
  type Filter,
  type ListedTask,
  type TasksList,
  type TasksModel,
  bar,
  barEntry,
  lists,
} from "./TasksModel";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Tasks Context: the Active list, the Backlog and the Filters as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. The circle marks the Task Done, and a toast
 * offers to Revert a Close, in the edit view while that is shown. A Parent
 * shows how many of its Subtasks are Done, a Subtask its Parent's Title,
 * before the Tags; rows are dragged to reorder. A row opens the edit view; its
 * menu (right click, long press) plans, moves or Rejects. The Filters entry of
 * the bar lists the Filters, with how many Tasks each matches; opening one shows
 * its Tasks as Active's rows, under a way back. A port of the Swift
 * `TasksView`, in native elements.
 */
export class TasksView {
  readonly element = make("main", "tasks");
  private readonly switch = make("div", "tasks-switch");
  private readonly switchButtons = new Map<string, HTMLButtonElement>();
  private readonly filterHeader = make("div", "tasks-filter-header");
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty");
  private readonly failure = make("p", "tasks-failure");
  private readonly toast = make("div", "tasks-toast");
  private readonly menu: RowMenu;
  private readonly drag: RowDrag<ListedTask>;
  private readonly filterDrag: RowDrag<Filter>;
  private readonly create = button("tasks-new", "", () => this.createNew());
  /** The edit view, while shown, and the Task it is of. */
  private editor: { dialog: HTMLDialogElement; task: string; refresh: () => void } | undefined;

  constructor(
    private readonly model: TasksModel,
    private readonly places: TaskPlaces,
  ) {
    this.switch.setAttribute("role", "radiogroup");
    this.switch.setAttribute("aria-label", "List");
    for (const { title, list } of bar) {
      const choose = button("tasks-switch-button", title, () => (model.list = list));
      choose.setAttribute("role", "radio");
      this.switchButtons.set(title, choose);
      this.switch.append(choose);
    }
    onNewKey(this.element, () => this.createNew());
    this.create.innerHTML = plusGlyph;
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Tasks");
    this.empty.setAttribute("role", "status");
    this.toast.setAttribute("role", "status");
    this.menu = new RowMenu(this.element);
    this.drag = new RowDrag((task, before) => model.moveBefore(task, before));
    this.filterDrag = new RowDrag((filter, before) => model.moveFilter(filter, before));
    const scroll = make("div", "tasks-scroll", [this.filterHeader, this.list, this.empty]);
    this.drag.attachEnd(scroll);
    this.filterDrag.attachEnd(scroll);
    this.element.append(
      make("header", "tasks-header", [this.switch]),
      scroll,
      this.failure,
      this.toast,
      this.create,
      this.menu.element,
    );
    model.onChange = () => this.update();
    this.update();
  }

  /** Shows the list as the model has it. */
  update(): void {
    const { model } = this;
    const { list } = model;
    const entry = barEntry(list);
    for (const [title, choose] of this.switchButtons) {
      choose.setAttribute("aria-checked", String(title === entry));
    }
    this.menu.hide();
    this.showList(list);
    this.showToast();
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
    this.placeToast();
    this.editor?.refresh();
  }

  /** "+" and the "n" key: a new Filter in the Filters list, a new Task elsewhere. */
  private createNew(): void {
    if (this.model.list.kind === "filters") filterDialog(this.model, this.element);
    else newTaskDialog(this.model, this.element);
  }

  /** The rows of the list shown, what an empty one says, and the "+" that fits. */
  private showList(list: TasksList): void {
    const { model } = this;
    const filters = list.kind === "filters";
    const label = filters ? "New Filter" : "New Task";
    this.create.setAttribute("aria-label", label);
    this.create.title = label;
    this.list.setAttribute("aria-label", filters ? "Filters" : "Tasks");
    this.filterHeader.hidden = list.kind !== "filter";
    this.filterHeader.replaceChildren(
      ...(model.filter
        ? [
            button("tasks-back", "Filters", () => (model.list = { kind: "filters" })),
            make("h2", "tasks-filter-title", [model.filter.title]),
          ]
        : []),
    );
    const rows = filters ? this.filterList() : model.tasks.map((task) => this.row(task));
    this.list.replaceChildren(...rows);
    this.empty.hidden = rows.length > 0;
    const [heading, text] = this.emptyText(list);
    this.empty.replaceChildren(make("strong", "", [heading]), make("span", "", [text]));
    if (filters) this.empty.append(button("tasks-empty-new", "New Filter", () => this.createNew()));
  }

  private emptyText(list: TasksList): [string, string] {
    switch (list.kind) {
      case "planning":
        return ["Tasks", lists.find((l) => l.planning === list.planning)!.empty];
      case "filters":
        return ["Filters", "No Filters yet."];
      case "filter":
        return ["Tasks", "No Active Tasks match."];
    }
  }

  private filterList(): HTMLElement[] {
    const { model, element } = this;
    return filterRows(
      model.filters,
      {
        open: (filter) => (model.list = { kind: "filter", id: filter.id }),
        edit: (filter) => filterDialog(model, element, filter),
        remove: (filter) => deleteFilterDialog(model, element, filter),
      },
      this.menu,
      this.filterDrag,
    );
  }

  /** A modal dialog covers the list's toast, so while the edit view is shown the toast is in it. */
  private placeToast(): void {
    const host = this.editor?.dialog ?? this.element;
    if (this.toast.parentElement !== host) host.append(this.toast);
  }

  /**
   * The edit view is shown while the model has a Task to edit, and is made
   * anew when that is another one; leaving it, however, leaves the Task's address.
   */
  private showEditor(): void {
    const { editing } = this.model;
    if (this.editor?.task === editing?.id) return;
    const shown = this.editor;
    this.editor = undefined;
    shown?.dialog.close();
    if (!editing) return;
    const { dialog, refresh } = editTaskDialog(this.model, editing, this.element, this.places);
    dialog.addEventListener("close", () => {
      // Closed by the model's say-so (the address left the Task): nothing to tell.
      if (this.editor?.dialog !== dialog) return;
      this.editor = undefined;
      this.placeToast(); // the dialog is removed with what is in it
      this.places.close();
    });
    this.editor = { dialog, task: editing.id, refresh };
  }

  /** Names the Task just Closed and offers to Revert it. */
  private showToast(): void {
    const { pendingClose } = this.model;
    this.toast.hidden = pendingClose === undefined;
    if (!pendingClose) return;
    const verb = pendingClose.closing === Closing.Done ? "done" : "rejected";
    this.toast.replaceChildren(
      make("span", "tasks-toast-text", [`${pendingClose.title} ${verb}`]),
      button("tasks-toast-revert", "Revert", () => this.model.revert()),
    );
  }

  private row(task: ListedTask): HTMLElement {
    const { line, open } = taskLine(task, this.model, () => this.places.open(task.id));
    open.setAttribute("aria-haspopup", "menu");
    const row = make("li", "task-row", [line]);
    this.menu.attach(row, () => this.menuItems(task), open);
    this.drag.attachRow(row, task);
    return row;
  }

  private menuItems(task: ListedTask): RowMenuItem[] {
    // A pending Done is Reverted at its circle.
    if (task.done) return [];
    const { model } = this;
    const items: RowMenuItem[] = [];
    // A Subtask follows its top-level Task's list.
    if (task.parent === undefined) {
      items.push(
        task.planning === Planning.Backlog
          ? {
              label: "Make Active",
              run: () => model.plan(task, Planning.Active),
            }
          : {
              label: "Move to Backlog",
              run: () => model.plan(task, Planning.Backlog),
            },
      );
    } else {
      items.push({
        label: "Make Top-Level",
        run: () => model.moveBefore(task, undefined),
      });
    }
    items.push({
      label: "Reject",
      run: () => model.close(task, Closing.Rejected),
      danger: true,
    });
    return items;
  }
}
