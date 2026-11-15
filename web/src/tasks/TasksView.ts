import { Closing, Planning } from "../core/Core";
import { button, make } from "../Dom";
import { RowDrag } from "./RowDrag";
import { tagLabel } from "./TagLabel";
import { RowMenu, type RowMenuItem } from "./RowMenu";
import { type TaskPlaces, editTaskDialog, newTaskDialog } from "./TaskDialogs";
import { type ListedTask, type TasksModel, lists } from "./TasksModel";

const subtasksGlyph = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m4 7 2 2 3-3M4 17l2 2 3-3M13 8h7M13 18h7"/></svg>`;
const parentGlyph = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M5 5v8a2 2 0 0 0 2 2h11m-4-4 4 4-4 4"/></svg>`;
const lockGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true"><path d="M7 10V8a5 5 0 0 1 10 0v2h1a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zm2 0h6V8a3 3 0 0 0-6 0z"/></svg>`;
const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Tasks Context: the Active list and the Backlog as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. The circle marks the Task Done, and a toast
 * offers to Revert a Close. A Parent shows how many of its Subtasks are Done,
 * a Subtask its Parent's Title, before the Tags; rows are dragged to reorder. A row opens the edit view; its
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
  /** The edit view, while shown, and the Task it is of. */
  private editor: { dialog: HTMLDialogElement; task: string } | undefined;

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
    const dialog = editTaskDialog(this.model, editing, this.element, this.places);
    dialog.addEventListener("close", () => {
      // Closed by the model's say-so (the address left the Task): nothing to tell.
      if (this.editor?.dialog !== dialog) return;
      this.editor = undefined;
      this.places.close();
    });
    this.editor = { dialog, task: editing.id };
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
    const circle = make("button", `task-circle priority-${task.priority}`);
    circle.type = "button";
    circle.classList.toggle("done", task.done);
    circle.setAttribute("aria-label", task.done ? "Revert Done" : "Done");
    circle.addEventListener("click", () => model.toggleDone(task));
    const title = make("span", "task-title", [task.title]);
    const text = make("span", "task-text", [title]);
    if (task.description) text.append(make("span", "task-description", [task.description]));
    const marks = this.markers(task);
    if (marks.length > 0 || task.tags.length > 0) {
      const labels = task.tags.map((tag) => tagLabel(tag, task.tagColors[tag]));
      // Separate nodes of one line, by name, without chips.
      const tags = make("span", "task-tags", labels.flatMap((label) => [label, " "]).slice(0, -1));
      text.append(make("span", "task-meta", [...marks, tags]));
    }
    const open = make("button", "task-open", [text]);
    open.type = "button";
    open.setAttribute("aria-haspopup", "menu");
    open.addEventListener("click", () => this.places.open(task.id));
    const line = make("div", "task-line", [circle, open]);
    if (task.blocked) {
      const lock = make("span", "task-blocked");
      lock.innerHTML = lockGlyph;
      lock.setAttribute("role", "img");
      lock.setAttribute("aria-label", "Blocked");
      line.append(lock);
    }
    const row = make("li", "task-row", [line]);
    this.menu.attach(row, () => this.menuItems(task), open);
    this.drag.attachRow(row, task);
    return row;
  }

  /** The Parent marker ("Done/total") and the Subtask marker (its Parent's Title) of a row. */
  private markers(task: ListedTask): HTMLElement[] {
    const marks: HTMLElement[] = [];
    if (task.subtasksTotal > 0) {
      const mark = make("span", "task-mark", [`${task.subtasksDone}/${task.subtasksTotal}`]);
      mark.insertAdjacentHTML("afterbegin", subtasksGlyph);
      mark.title = `${task.subtasksDone} of ${task.subtasksTotal} Subtasks Done`;
      marks.push(mark);
    }
    if (task.parentTitle !== undefined) {
      const mark = make("span", "task-mark task-parent", [task.parentTitle]);
      mark.insertAdjacentHTML("afterbegin", parentGlyph);
      mark.title = `Subtask of ${task.parentTitle}`;
      marks.push(mark);
    }
    return marks;
  }

  private menuItems(task: ListedTask): RowMenuItem[] {
    // A pending Done is Reverted at its circle.
    if (task.done) return [];
    const { model } = this;
    const items: RowMenuItem[] = [
      {
        label: "Add Subtask",
        run: () => newTaskDialog(model, this.element, task),
      },
    ];
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
