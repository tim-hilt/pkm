import { Closing, TaskQuickAdd } from "../core/Core";
import { button, make } from "../Dom";
import { QuickAddField } from "./QuickAddField";
import { RowDrag } from "./RowDrag";
import { RowMenu } from "./RowMenu";
import { taskLine } from "./TaskRow";
import type { TasksModel } from "./TasksModel";

const chevronGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`;

/**
 * A Task's Subtasks in its edit view: a header with the "Done/total" counter
 * (none without Subtasks) and a fold button, then the direct Subtasks as list
 * rows, then an "Add Subtask" button that turns into a focused Quick Add field:
 * Enter adds the Subtask at the end and empties the field, which keeps the focus;
 * Enter on an empty field, or blur, closes it. A click opens a Subtask via
 * `open`. A row's circle marks it Done or Reverts that; its menu (right click,
 * long press) holds only Reject; a mouse drags it to reorder among its siblings. Starts unfolded; the fold is not saved; folding hides the button too.
 * `refresh` redraws it from the model, as last saved.
 */
export class SubtasksSection {
  readonly element = make("section", "tasks-subtasks");
  private readonly counter = make("span", "tasks-subtasks-count");
  private readonly fold = make("button", "tasks-subtasks-fold");
  private readonly rows = make("ul", "tasks-subtasks-list");
  private readonly addButton = button("tasks-subtasks-add-button", "Add Subtask", () =>
    this.startAdding(),
  );
  /** Holds the "Add Subtask" button, or the Quick Add field it turned into. */
  private readonly adder = make("div", "tasks-subtasks-add", [this.addButton]);
  /** Its own, so that rows only reorder among the section's. */
  private readonly drag = new RowDrag((subtask, before) => this.model.moveSubtask(subtask, before));
  private readonly menu = new RowMenu(document.body);
  private readonly body = make("div", "", [this.rows, this.adder]);
  /** The open Quick Add field and the Shortcuts it owns, until closed. */
  private adding: { field: QuickAddField; quickAdd: TaskQuickAdd } | undefined;
  private folded = false;

  constructor(
    private readonly model: TasksModel,
    private readonly taskId: string,
    private readonly open: (id: string) => void,
  ) {
    this.fold.type = "button";
    this.fold.innerHTML = chevronGlyph;
    this.fold.addEventListener("click", () => {
      this.folded = !this.folded;
      this.refresh();
    });
    this.element.append(
      make("header", "tasks-subtasks-header", [
        make("h3", "", ["Subtasks"]),
        this.counter,
        this.fold,
      ]),
      this.body,
      this.menu.element,
    );
    this.drag.attachEnd(this.adder);
    this.refresh();
  }

  /** Turns the Quick Add field, if open, back into the "Add Subtask" button and frees its Quick Add. */
  stopAdding(): void {
    const { adding } = this;
    if (!adding) return;
    // Cleared first: removing the focused field blurs it, which calls this again.
    this.adding = undefined;
    this.adder.replaceChildren(this.addButton);
    adding.quickAdd.free();
  }

  private startAdding(): void {
    const quickAdd = new TaskQuickAdd();
    const field = new QuickAddField(this.model, quickAdd, {
      submit: () => this.submit(),
      blur: () => this.stopAdding(),
    });
    this.adding = { field, quickAdd };
    this.adder.replaceChildren(field.element);
    field.focus();
  }

  private submit(): void {
    const { adding, model } = this;
    const parent = model.find(this.taskId);
    if (!adding || !parent) return;
    const { field, quickAdd } = adding;
    if (field.title === "") return this.stopAdding();
    const extras = { priority: quickAdd.priority(), tags: quickAdd.tags() };
    if (model.addSubtask(field.title, parent, extras)) field.clear();
  }

  refresh(): void {
    const { model } = this;
    // The counts as last saved: the Subtasks' own edits change them, not this view's.
    const { subtasksDone: done = 0, subtasksTotal: total = 0 } = model.find(this.taskId) ?? {};
    this.counter.hidden = total === 0;
    this.counter.textContent = `${done}/${total}`;
    this.counter.title = `${done} of ${total} Subtasks Done`;
    this.fold.setAttribute("aria-label", this.folded ? "Unfold Subtasks" : "Fold Subtasks");
    this.fold.setAttribute("aria-expanded", String(!this.folded));
    this.fold.classList.toggle("folded", this.folded);
    this.body.hidden = this.folded;
    this.menu.hide();
    this.rows.replaceChildren(
      ...model.subtasks(this.taskId).map((subtask) => {
        const { line, open } = taskLine(subtask, model, () => this.open(subtask.id), {
          showsParent: false,
        });
        open.setAttribute("aria-haspopup", "menu");
        const row = make("li", "task-row", [line]);
        // A pending Done is Reverted at its circle, so it has no menu.
        const items = subtask.done
          ? []
          : [{ label: "Reject", run: () => model.close(subtask, Closing.Rejected), danger: true }];
        this.menu.attach(row, () => items, open);
        this.drag.attachRow(row, subtask);
        return row;
      }),
    );
  }
}
