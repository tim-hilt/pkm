import { make } from "../Dom";
import { taskLine } from "./TaskRow";
import type { TasksModel } from "./TasksModel";

const chevronGlyph = `<svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m6 9 6 6 6-6"/></svg>`;

/**
 * A Task's Subtasks in its edit view: a header with the "Done/total" counter
 * (none without Subtasks) and a fold button, then the direct Subtasks as list
 * rows. A click opens a Subtask via `open`. Starts unfolded; the fold is not
 * saved. `refresh` redraws it from the model, as last saved.
 */
export class SubtasksSection {
  readonly element = make("section", "tasks-subtasks");
  private readonly counter = make("span", "tasks-subtasks-count");
  private readonly fold = make("button", "tasks-subtasks-fold");
  private readonly rows = make("ul", "tasks-subtasks-list");
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
      this.rows,
    );
    this.refresh();
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
    this.rows.hidden = this.folded;
    this.rows.replaceChildren(
      ...model.subtasks(this.taskId).map((subtask) => {
        const { line } = taskLine(subtask, model, () => this.open(subtask.id), false);
        return make("li", "task-row", [line]);
      }),
    );
  }
}
