import { Closing, Planning } from "../core/Core";
import { button, make } from "../Dom";
import { RowDrag } from "./RowDrag";
import { RowMenu, type RowMenuItem } from "./RowMenu";
import { editTaskDialog, newTaskDialog } from "./TaskDialogs";
import { type ListedTask, type TasksModel, lists } from "./TasksModel";

const chevronGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m9 6 6 6-6 6"/></svg>`;
const lockGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M7 10V8a5 5 0 0 1 10 0v2h1a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zm2 0h6V8a3 3 0 0 0-6 0z"/></svg>`;
const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/** How the view opens and leaves a Task's edit view, which has an address of its own. */
export interface TaskPlaces {
  open(id: string): void;
  close(): void;
}

/**
 * The Tasks Context: the Active list and the Backlog as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. The circle marks the Task Done, and a toast
 * offers to Revert a Close. Subtasks are indented and collapse under their
 * Parent; rows are dragged to reorder and move. A row opens the edit view; its
 * menu (right click, long press) adds a Subtask, plans, moves or Rejects. A
 * port of the Swift `TasksView`, in native elements.
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
  /** The edit view, while shown. */
  private editor: HTMLDialogElement | undefined;

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
    const create = button("tasks-new", "", () => newTaskDialog(model, this.element));
    create.innerHTML = plusGlyph;
    create.setAttribute("aria-label", "New Task");
    create.title = "New Task";
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Tasks");
    this.empty.setAttribute("role", "status");
    this.toast.setAttribute("role", "status");
    this.menu = new RowMenu(this.element);
    this.drag = new RowDrag(model);
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
    this.list.replaceChildren(...model.rows.map((task) => this.row(task)));
    this.showToast();
    this.empty.hidden = model.tasks.length > 0;
    this.empty.replaceChildren(
      make("strong", "", ["Tasks"]),
      make("span", "", [lists.find((l) => l.planning === model.planning)!.empty]),
    );
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
  }

  /** The edit view is shown while the model has a Task to edit; leaving it, however, leaves the Task's address. */
  private showEditor(): void {
    const { editing } = this.model;
    if (!editing) {
      const shown = this.editor;
      this.editor = undefined;
      return shown?.close();
    }
    if (this.editor) return;
    const editor = editTaskDialog(this.model, editing, this.element);
    editor.addEventListener("close", () => {
      // Closed by the model's say-so (the address left the Task): nothing to tell.
      if (this.editor !== editor) return;
      this.editor = undefined;
      this.places.close();
    });
    this.editor = editor;
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
    const { model } = this;
    const disclose = make("button", "task-disclose");
    disclose.type = "button";
    disclose.innerHTML = chevronGlyph;
    disclose.disabled = !task.hasSubtasks;
    disclose.setAttribute("aria-label", model.isCollapsed(task) ? "Expand" : "Collapse");
    disclose.setAttribute("aria-expanded", String(!model.isCollapsed(task)));
    disclose.classList.toggle("collapsed", model.isCollapsed(task));
    disclose.addEventListener("click", () => model.toggleCollapsed(task));
    const circle = make("button", `task-circle priority-${task.priority}`);
    circle.type = "button";
    circle.classList.toggle("done", task.done);
    circle.disabled = task.closed;
    circle.setAttribute("aria-label", task.done ? "Revert Done" : "Done");
    circle.addEventListener("click", () => model.toggleDone(task));
    const title = make("span", "task-title", [task.title]);
    const text = make("span", "task-text", [title]);
    if (task.description) text.append(make("span", "task-description", [task.description]));
    if (task.tags.length > 0) text.append(make("span", "task-tags", [task.tags.join(" ")]));
    const open = make("button", "task-open", [text]);
    open.type = "button";
    open.setAttribute("aria-haspopup", "menu");
    open.addEventListener("click", () => this.places.open(task.id));
    open.disabled = task.closed;
    const line = make("div", "task-line", [disclose, circle, open]);
    if (task.blocked) {
      const lock = make("span", "task-blocked");
      lock.innerHTML = lockGlyph;
      lock.setAttribute("role", "img");
      lock.setAttribute("aria-label", "Blocked");
      line.append(lock);
    }
    line.style.paddingLeft = `${task.depth * 24}px`;
    const row = make("li", "task-row", [line]);
    row.classList.toggle("closed", task.closed);
    this.menu.attach(row, () => this.menuItems(task), open);
    this.drag.attachRow(row, task);
    return row;
  }

  private menuItems(task: ListedTask): RowMenuItem[] {
    // A pending Done is Reverted at its circle.
    if (task.closed || task.done) return [];
    const { model } = this;
    const items: RowMenuItem[] = [
      {
        label: "Add Subtask",
        run: () => newTaskDialog(model, this.element, task),
      },
    ];
    // A Subtask follows its top-level Task's list.
    if (task.depth === 0) {
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
