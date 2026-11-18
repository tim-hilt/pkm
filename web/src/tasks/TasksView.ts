import { Closing, Direction, FilterSortKey, sortKeys } from "../core/Core";
import { button, make, onNewKey } from "../Dom";
import { filterDialog, deleteFilterDialog } from "./FilterDialogs";
import { filterRows } from "./FiltersList";
import { RowDrag } from "./RowDrag";
import { RowMenu, type RowMenuItem } from "./RowMenu";
import { taskLine } from "./TaskRow";
import { type TaskPlaces, editTaskDialog, newTaskDialog } from "./TaskDialogs";
import { type Filter, type ListedTask, type TasksList, type TasksModel } from "./TasksModel";

const sortGlyph = `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 4v16M4 16l4 4 4-4M16 20V4M12 8l4-4 4 4"/></svg>`;

const sortKeyLabels = { [FilterSortKey.Manual]: "Manual", [FilterSortKey.Priority]: "Priority" };
const directionLabels = {
  [Direction.Ascending]: "Ascending",
  [Direction.Descending]: "Descending",
};

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Tasks Context: the Filters and the Tasks they match as Todoist-style rows: a
 * circle coloured by Priority, the Title, the Description faded on one line
 * and the Tags fainter still. The circle marks the Task Done, and a toast
 * offers to Revert a Close, in the edit view while that is shown. A Parent
 * shows how many of its Subtasks are Open, a Subtask its Parent's Title,
 * before the Tags; rows are dragged to reorder. A row opens the edit view; its
 * menu (right click, long press) Rejects. The Filters entry of
 * the bar lists the Favorites, each a Filter, and the Filters, with how many Tasks each matches; opening one shows
 * its Tasks as rows, under its Title, with a Sort menu in the toolbar above them. A port of the Swift
 * `TasksView`, in native elements.
 */
export class TasksView {
  readonly element = make("main", "tasks");
  private readonly switch = make("div", "tasks-switch");
  /** The bar as last built, to build it again only when its entries changed. */
  private barKey = "";
  /** The bar's entry last checked. */
  private checkedKey = "";
  private switchButtons: { list: TasksList; choose: HTMLButtonElement }[] = [];
  private readonly filterHeader = make("div", "tasks-filter-header");
  private readonly sort = button("tasks-sort", "", () => this.showSortMenu());
  private readonly toolbar = make("div", "tasks-toolbar", [this.sort]);
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
    onNewKey(this.element, () => this.createNew());
    this.create.innerHTML = plusGlyph;
    this.sort.innerHTML = sortGlyph;
    this.sort.title = "Sort";
    this.sort.setAttribute("aria-label", "Sort");
    this.sort.setAttribute("aria-haspopup", "menu");
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Tasks");
    this.empty.setAttribute("role", "status");
    this.toast.setAttribute("role", "status");
    this.menu = new RowMenu(this.element);
    this.drag = new RowDrag((task, before) => model.moveBefore(task, before));
    this.filterDrag = new RowDrag((filter, before) => model.moveFilter(filter, before));
    const scroll = make("div", "tasks-scroll", [
      this.filterHeader,
      this.toolbar,
      this.list,
      this.empty,
    ]);
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
    this.showBar();
    this.menu.hide();
    this.showList(list);
    this.showToast();
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
    this.placeToast();
    this.editor?.refresh();
  }

  /** The bar's entries, the one that shows the list checked and scrolled into view. */
  private showBar(): void {
    const { model } = this;
    const entries = model.bar;
    const key = JSON.stringify(entries);
    const current = JSON.stringify(model.barEntry);
    // Scrolled only when it changed, not on every update.
    const scroll = key !== this.barKey || current !== this.checkedKey;
    this.checkedKey = current;
    if (key !== this.barKey) {
      this.barKey = key;
      this.switchButtons = entries.map(({ title, list }) => {
        const choose = button("tasks-switch-button", title, () => (model.list = list));
        choose.setAttribute("role", "radio");
        return { list, choose };
      });
      this.switch.replaceChildren(...this.switchButtons.map(({ choose }) => choose));
    }
    for (const { list, choose } of this.switchButtons) {
      const checked = JSON.stringify(list) === current;
      choose.setAttribute("aria-checked", String(checked));
      // The bar scrolls sideways when the entries don't fit.
      if (checked && scroll) choose.scrollIntoView({ inline: "center", block: "nearest" });
    }
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
    // A Favorite is reached from the bar, any other Filter from the Filters list (the bar's Filters entry leads back).
    const opened = model.barEntry.kind === "filters" ? model.filter : undefined;
    this.filterHeader.hidden = opened === undefined;
    this.filterHeader.replaceChildren(
      ...(opened ? [make("h2", "tasks-filter-title", [opened.title])] : []),
    );
    this.toolbar.hidden = filters;
    const rows = filters ? this.filterList() : model.tasks.map((task) => this.row(task));
    this.list.replaceChildren(...rows);
    this.empty.hidden = rows.length > 0;
    const [heading, text] = this.emptyText(list);
    this.empty.replaceChildren(make("strong", "", [heading]), make("span", "", [text]));
    if (filters) this.empty.append(button("tasks-empty-new", "New Filter", () => this.createNew()));
  }

  /** The Sort menu of the Filter shown, under the Sort button: the keys, and Direction unless Manual. */
  private showSortMenu(): void {
    const { model } = this;
    const filter = model.filter;
    if (!filter) return;
    const { key: current, direction } = filter.sort;
    const keys: RowMenuItem[] = sortKeys().map((key) => ({
      label: sortKeyLabels[key],
      checked: key === current,
      run: () => model.setSort(filter, key, direction),
    }));
    const directions: RowMenuItem[] =
      direction === undefined
        ? []
        : [Direction.Ascending, Direction.Descending].map((d, i) => ({
            label: directionLabels[d],
            checked: d === direction,
            heading: i === 0 ? "Direction" : undefined,
            run: () => model.setSort(filter, current, d),
          }));
    const { left, bottom } = this.sort.getBoundingClientRect();
    this.menu.show([...keys, ...directions], left, bottom, this.sort);
  }

  private emptyText(list: TasksList): [string, string] {
    switch (list.kind) {
      case "filters":
        return ["Filters", "No Filters yet."];
      case "filter":
        return ["Tasks", "No Open Tasks match."];
    }
  }

  private filterList(): HTMLElement[] {
    const { model, element } = this;
    return filterRows(
      model.filters,
      {
        open: (filter) => (model.list = { kind: "filter", id: filter.id }),
        favorite: (filter) => model.setFavorite(filter, !filter.favorite),
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
    return [
      {
        label: "Reject",
        run: () => model.close(task, Closing.Rejected),
        danger: true,
      },
    ];
  }
}
