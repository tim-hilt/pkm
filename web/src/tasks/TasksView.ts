import { Planning } from "../core/Core";
import { button, make } from "../Dom";
import { RowMenu } from "./RowMenu";
import { editTaskDialog, newTaskDialog } from "./TaskDialogs";
import { type ListedTask, type TasksModel, lists } from "./TasksModel";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Tasks Context: the Active list and the Backlog as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. A row opens the edit view; its menu (right click,
 * long press) makes it Active or moves it to the Backlog. A port of the Swift
 * `TasksView`, in native elements.
 */
export class TasksView {
  readonly element = make("main", "tasks");
  private readonly switch = make("div", "tasks-switch");
  private readonly switchButtons = new Map<Planning, HTMLButtonElement>();
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty");
  private readonly failure = make("p", "tasks-failure");
  private readonly menu: RowMenu;

  constructor(private readonly model: TasksModel) {
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
    this.menu = new RowMenu(this.element);
    const scroll = make("div", "tasks-scroll", [this.list, this.empty]);
    this.element.append(
      make("header", "tasks-header", [this.switch]),
      scroll,
      this.failure,
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
    this.empty.hidden = model.tasks.length > 0;
    this.empty.replaceChildren(
      make("strong", "", ["Tasks"]),
      make("span", "", [lists.find((l) => l.planning === model.planning)!.empty]),
    );
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
  }

  private row(task: ListedTask): HTMLElement {
    const circle = make("span", `task-circle priority-${task.priority}`);
    circle.classList.toggle("done", task.done);
    const text = make("span", "task-text", [make("span", "task-title", [task.title])]);
    if (task.description) text.append(make("span", "task-description", [task.description]));
    if (task.tags.length > 0) text.append(make("span", "task-tags", [task.tags.join(" ")]));
    const open = make("button", "task-open", [circle, text]);
    open.type = "button";
    open.setAttribute("aria-haspopup", "menu");
    open.addEventListener("click", () => {
      if (!task.closed) editTaskDialog(this.model, task, this.element);
    });
    const row = make("li", "task-row", [open]);
    row.style.paddingLeft = `${task.depth * 24}px`;
    this.menu.attach(row, () => this.menuItems(task), open);
    return row;
  }

  private menuItems(task: ListedTask) {
    // A Subtask follows its top-level Task's list.
    if (task.closed || task.done || task.depth > 0) return [];
    const { model } = this;
    return task.planning === Planning.Backlog
      ? [{ label: "Make Active", run: () => model.plan(task, Planning.Active) }]
      : [{ label: "Move to Backlog", run: () => model.plan(task, Planning.Backlog) }];
  }
}
