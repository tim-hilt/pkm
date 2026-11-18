import { button, make } from "../Dom";
import { confirmButton, field, modal, textInput } from "./TaskDialogs";
import { type Filter, TasksModel } from "./TasksModel";
import { tagLabel } from "./TagLabel";

/**
 * Asks for a Title, a Query and a Group by: adds the Filter at the end of the Filters list,
 * or, given one, changes it. The Query field refuses what a Query can't hold as
 * it is typed, suggests Tags after `@`, shows why a Query can't be read, inline,
 * and marks Tags that don't exist, which doesn't stop saving. The Group-by field
 * (`@work @private`) suggests Tags, leaving out those listed, and shows why it
 * can't be saved, inline. Save is off while the Title is empty or the Query or
 * Group by is refused.
 */
export function filterDialog(model: TasksModel, host: HTMLElement, editing?: Filter): void {
  const title = textInput(editing?.title ?? "");
  title.setAttribute("aria-label", "Title");
  title.placeholder = "Title";
  const query = textInput(editing?.query ?? "");
  query.setAttribute("aria-label", "Query");
  query.placeholder = "@work & !@in-review";
  query.autocapitalize = "none";
  query.spellcheck = false;
  query.setAttribute("autocorrect", "off");
  // The email keyboard has "@" beside the space bar, as for Tags.
  query.inputMode = "email";
  const suggestions = make("ul", "tasks-query-suggestions");
  const groupBy = textInput(editing?.groupBy ?? "");
  groupBy.setAttribute("aria-label", "Group by");
  groupBy.placeholder = "@work @private";
  groupBy.autocapitalize = "none";
  groupBy.spellcheck = false;
  groupBy.setAttribute("autocorrect", "off");
  groupBy.inputMode = "email";
  const groupBySuggestions = make("ul", "tasks-query-suggestions");
  const groupByNote = make("p", "tasks-query-note error");
  groupByNote.setAttribute("role", "status");
  const note = make("p", "tasks-query-note");
  note.setAttribute("role", "status");
  const save = confirmButton(editing ? "Save" : "Add");
  const { dialog, form } = modal(
    host,
    editing ? "Edit Filter" : "New Filter",
    [
      field("Title", title),
      field("Query", query),
      suggestions,
      note,
      field("Group by", groupBy),
      groupBySuggestions,
      groupByNote,
    ],
    [button("", "Cancel", () => dialog.close()), save],
  );

  const show = () => {
    const { problem, unknown } = model.checkQuery(query.value);
    save.disabled = title.value.trim() === "" || problem !== undefined;
    note.classList.toggle("error", problem !== undefined && query.value !== "");
    note.hidden = true;
    if (problem && query.value !== "") {
      note.textContent = problem.message;
      note.hidden = false;
    } else if (unknown.length > 0) {
      note.textContent = `Unknown: ${unknown.map((tag) => `@${tag}`).join(", ")}`;
      note.hidden = false;
    }
    suggest(suggestions, query, model.suggestQueryTags(query.value));
    const refused = model.checkGroupBy(groupBy.value);
    groupByNote.hidden = refused === undefined;
    groupByNote.textContent = refused ?? "";
    save.disabled ||= refused !== undefined;
    suggest(groupBySuggestions, groupBy, model.suggestGroupByTags(groupBy.value));
  };
  /** Fills `list` with a button per Tag, each completing the Tag being typed in `input`. */
  const suggest = (list: HTMLElement, input: HTMLInputElement, tags: string[]) => {
    const colors = new Map(model.allTags.map((t) => [t.name, t.color]));
    list.replaceChildren(
      ...tags.map((tag) => {
        const pick = button("tasks-query-suggestion", "", () => {
          input.value = TasksModel.completingQuery(input.value, tag);
          show();
          input.focus();
        });
        pick.append(tagLabel(tag, colors.get(tag)));
        // Keeps the focus in the field.
        pick.addEventListener("mousedown", (event) => event.preventDefault());
        return make("li", "", [pick]);
      }),
    );
  };
  query.addEventListener("input", () => {
    const typed = query.value;
    const allowed = TasksModel.allowedQueryText(typed);
    if (allowed === typed) return;
    const caret = (query.selectionStart ?? typed.length) - (typed.length - allowed.length);
    query.value = allowed;
    query.setSelectionRange(caret, caret);
  });
  form.addEventListener("input", show);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (save.disabled) return;
    const saved = editing
      ? model.editFilter(editing.id, title.value, query.value, groupBy.value)
      : model.addFilter(title.value, query.value, groupBy.value);
    if (saved) dialog.close();
  });
  show();
  (editing ? query : title).focus();
}

/** Asks to Delete a Filter; no Task changes. Cancel changes nothing. */
export function deleteFilterDialog(model: TasksModel, host: HTMLElement, filter: Filter): void {
  const confirm = confirmButton("Delete");
  confirm.classList.add("danger");
  const { dialog, form } = modal(
    host,
    `Delete “${filter.title}”?`,
    [],
    [button("", "Cancel", () => dialog.close()), confirm],
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    model.deleteFilter(filter);
    dialog.close();
  });
  confirm.focus();
}
