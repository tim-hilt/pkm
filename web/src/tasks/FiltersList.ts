import { make } from "../Dom";
import { RowDrag } from "./RowDrag";
import type { RowMenu, RowMenuItem } from "./RowMenu";
import type { Filter } from "./TasksModel";

/**
 * The rows of the Filters list: each Filter with its Title, its Query (or why
 * it can't be read) and how many Open Tasks it matches. A row opens the
 * Filter; its menu (right click, long press) marks it as a Favorite (starred), edits or Deletes it; rows are
 * dragged to reorder, by the `drag` of the view.
 */
export function filterRows(
  filters: readonly Filter[],
  actions: {
    open: (filter: Filter) => void;
    favorite: (filter: Filter) => void;
    edit: (filter: Filter) => void;
    remove: (filter: Filter) => void;
  },
  menu: RowMenu,
  drag: RowDrag<Filter>,
): HTMLElement[] {
  return filters.map((filter) => {
    const title = make("span", "task-title", [filter.title]);
    if (filter.favorite) {
      const star = make("span", "filter-star", ["★"]);
      star.setAttribute("role", "img");
      star.setAttribute("aria-label", "Favorite");
      title.append(" ", star);
    }
    const text = make("span", "task-text", [title]);
    const detail = make("span", "task-description filter-query", [filter.error ?? filter.query]);
    detail.classList.toggle("error", filter.error !== undefined);
    text.append(detail);
    const open = make("button", "task-open", [text]);
    open.type = "button";
    open.setAttribute("aria-haspopup", "menu");
    open.addEventListener("click", () => actions.open(filter));
    const line = make("div", "task-line", [open]);
    if (filter.error === undefined) {
      const count = make("span", "filter-count", [String(filter.matches)]);
      count.setAttribute("aria-label", `${filter.matches} Open Tasks`);
      line.append(count);
    }
    const row = make("li", "task-row", [line]);
    const items = (): RowMenuItem[] => [
      {
        label: filter.favorite ? "Unfavorite" : "Favorite",
        run: () => actions.favorite(filter),
      },
      { label: "Edit", run: () => actions.edit(filter) },
      { label: "Delete", run: () => actions.remove(filter), danger: true },
    ];
    menu.attach(row, items, open);
    drag.attachRow(row, filter);
    return row;
  });
}
