import { isValidTag } from "../core/Core";
import { button, make } from "../Dom";
import { tagLabel } from "./TagLabel";
import { TasksModel } from "./TasksModel";

let nextId = 0;

/** What the picker does with the pick: save all the Tags checked on Done, or one Tag at once. */
export type TagPicking =
  | { kind: "many"; done: (tags: string[]) => void }
  | { kind: "one"; toggle: (tag: string) => void };

/**
 * A modal list of every known Tag with a checkbox each, and a search input at
 * the top that takes the focus on opening and also offers to create a Tag.
 * "many": Done, at the bottom right, calls `done` with the Tags checked and
 * closes. "one": Cancel instead of Done; checking or unchecking a Tag, or
 * creating one, calls `toggle` with it and closes. Cancel and Esc change nothing.
 */
export function pickTagsDialog(
  parent: HTMLElement,
  model: Pick<TasksModel, "allTags">,
  chosen: string[],
  picking: TagPicking,
): void {
  const selected = [...chosen];
  const known = model.allTags;
  const colors = new Map(known.map((t) => [t.name, t.color]));
  const created: string[] = [];
  const dialog = make("dialog", "tasks-dialog tasks-picker");
  const heading = make("h2", "", ["Tags"]);
  dialog.setAttribute("aria-labelledby", (heading.id = `tasks-picker-${nextId++}`));
  const list = make("ul", "tasks-picker-list");
  const hint = make("p", "tasks-hint", ["Tags use a–z, 0–9, - and / only."]);
  hint.hidden = true;
  const search = make("input", "tasks-input");
  search.type = "search";
  search.placeholder = search.ariaLabel = "Search or create";
  search.autocapitalize = "off";
  search.autocomplete = "off";
  search.spellcheck = false;
  // showModal focuses it, which opens the keyboard on touch devices.
  search.autofocus = true;

  /** A Tag checked or unchecked: "one" toggles it and closes, "many" only records it. */
  const pick = (tag: string, checked: boolean) => {
    if (picking.kind === "one") {
      picking.toggle(tag);
      dialog.close();
    } else if (checked) selected.push(tag);
    else selected.splice(selected.indexOf(tag), 1);
  };
  const creatable = () =>
    isValidTag(search.value) && !colors.has(search.value) && !created.includes(search.value);
  const create = () => {
    pick(search.value, true);
    if (picking.kind === "one") return;
    created.push(search.value);
    search.value = "";
    show();
  };
  const show = () => {
    // Known ones by Relevance.
    const names = [...created, ...known.map((t) => t.name)].filter((n) => n.includes(search.value));
    const rows = names.map((tag) => {
      const box = make("input", "");
      box.type = "checkbox";
      box.checked = selected.includes(tag);
      box.addEventListener("change", () => pick(tag, box.checked));
      return make("li", "", [make("label", "", [tagLabel(tag, colors.get(tag)), box])]);
    });
    if (creatable()) {
      rows.unshift(
        make("li", "", [button("tasks-suggestion", `Create Tag '${search.value}'`, create)]),
      );
    }
    list.replaceChildren(...rows);
  };
  search.addEventListener("input", () => {
    const allowed = TasksModel.allowedTagText(search.value);
    hint.hidden = allowed === search.value;
    search.value = allowed;
    show();
  });
  search.addEventListener("keydown", (event) => {
    if (event.key !== "Enter") return;
    event.preventDefault();
    if (creatable()) create();
  });
  dialog.append(heading, search, list, hint);
  // Without Esc (touch), Cancel is the only way out of "one", which has no Done.
  const action =
    picking.kind === "one"
      ? button("", "Cancel", () => dialog.close())
      : button("tasks-confirm", "Done", () => {
          picking.done(selected);
          dialog.close();
        });
  dialog.append(make("div", "tasks-actions", [action]));
  dialog.addEventListener("close", () => dialog.remove());
  show();
  parent.append(dialog);
  dialog.showModal();
}
