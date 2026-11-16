import { Closing, Planning } from "../core/Core";
import { button, make, onNewKey } from "../Dom";
import { RowDrag } from "./RowDrag";
import { RowMenu, type RowMenuItem } from "./RowMenu";
import { taskLine } from "./TaskRow";
import { type TaskPlaces, editTaskDialog, newTaskDialog } from "./TaskDialogs";
import { type ListedTask, type TasksModel, lists } from "./TasksModel";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Tasks Context: the Active list and the Backlog as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. The circle marks the Task Done, and a toast
 * offers to Revert a Close, in the edit view while that is shown. A Parent
 * shows how many of its Subtasks are Done, a Subtask its Parent's Title,
 * before the Tags; rows are dragged to reorder. A row opens the edit view; its
 * menu (right click, long press) plans, moves or Rejects. A port of the Swift
 * `TasksView`, in native elements.
 */
export class TasksView {
  readonly element = make("main", "tasks");
  private readonly switch = make("div", "tasks-switch");
  private readonly switchButtons = new Map<Planning, HTMLButtonElement>();
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty");
  private readonly failure = make("p", "tasks-failure");
  private readonly toast = make("div", "tasks-toast");
  private readonly menu: RowMenu;
  private readonly drag: RowDrag;
  /** The edit view, while shown, and the Task it is of. */
  private editor: { dialog: HTMLDialogElement; task: string; refresh: () => void } | undefined;

  constructor(
    private readonly model: TasksModel,
    private readonly places: TaskPlaces,
  ) {
    this.switch.setAttribute("role", "radiogroup");
    this.switch.setAttribute("aria-label", "List");
    for (const { planning, title } of lists) {
      const choose = button("tasks-switch-button", title, () => (model.planning = planning));
      choose.setAttribute("role", "radio");
      this.switchButtons.set(planning, choose);
      this.switch.append(choose);
    }
    const newTask = () => newTaskDialog(model, this.element);
    const create = button("tasks-new", "", newTask);
    onNewKey(this.element, newTask);
    create.innerHTML = plusGlyph;
    create.setAttribute("aria-label", "New Task");
    create.title = "New Task";
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Tasks");
    this.empty.setAttribute("role", "status");
    this.toast.setAttribute("role", "status");
    this.menu = new RowMenu(this.element);
    this.drag = new RowDrag((task, before) => model.moveBefore(task, before));
    const scroll = make("div", "tasks-scroll", [this.list, this.empty]);
    this.drag.attachEnd(scroll);
    this.element.append(
      make("header", "tasks-header", [this.switch]),
      scroll,
      this.failure,
      this.toast,
      create,
      this.menu.element,
    );
    model.onChange = () => this.update();
    this.update();
  }

  /** Shows the list as the model has it. */
  update(): void {
    const { model } = this;
    for (const [planning, choose] of this.switchButtons) {
      choose.setAttribute("aria-checked", String(planning === model.planning));
    }
    this.menu.hide();
    this.list.replaceChildren(...model.tasks.map((task) => this.row(task)));
    this.showToast();
    this.empty.hidden = model.tasks.length > 0;
    this.empty.replaceChildren(
      make("strong", "", ["Tasks"]),
      make("span", "", [lists.find((l) => l.planning === model.planning)!.empty]),
    );
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
    this.placeToast();
    this.editor?.refresh();
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
