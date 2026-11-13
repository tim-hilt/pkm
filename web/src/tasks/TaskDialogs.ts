import { button, make } from "../Dom";
import { type Blocker, type ListedTask, TasksModel, lists } from "./TasksModel";

let nextId = 0;

/** A modal `<dialog>` with a form that is saved with its confirm button or Enter, and gone once closed. */
function openDialog(
  parent: HTMLElement,
  title: string,
  confirm: string,
  fields: HTMLElement[],
  save: () => boolean,
  canSave: () => boolean = () => true,
): void {
  const dialog = make("dialog", "tasks-dialog");
  const heading = make("h2", "", [title]);
  dialog.setAttribute("aria-labelledby", (heading.id = `tasks-dialog-${nextId++}`));
  const form = make("form", "tasks-form");
  const confirmButton = make("button", "tasks-confirm", [confirm]);
  confirmButton.type = "submit";
  const actions = make("div", "tasks-actions", [
    button("", "Cancel", () => dialog.close()),
    confirmButton,
  ]);
  form.append(heading, ...fields, actions);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (save()) dialog.close();
  });
  form.addEventListener("input", () => (confirmButton.disabled = !canSave()));
  confirmButton.disabled = !canSave();
  dialog.append(form);
  dialog.addEventListener("close", () => dialog.remove());
  parent.append(dialog);
  dialog.showModal();
}

function field(label: string, control: HTMLElement): HTMLLabelElement {
  return make("label", "tasks-field", [make("span", "", [label]), control]);
}

function textInput(value: string): HTMLInputElement {
  const input = make("input", "tasks-input");
  input.type = "text";
  input.value = value;
  return input;
}

/** A row of radio buttons, one chosen; `change` is called with the value picked. */
function choice<T extends string | number>(
  label: string,
  options: { value: T; text: string }[],
  chosen: T,
  change: (value: T) => void,
): HTMLElement {
  const name = `tasks-choice-${nextId++}`;
  const group = make("fieldset", "tasks-choice");
  group.append(make("legend", "", [label]));
  for (const { value, text } of options) {
    const input = make("input", "");
    input.type = "radio";
    input.name = name;
    input.checked = value === chosen;
    input.addEventListener("change", () => change(value));
    group.append(make("label", "", [input, make("span", "", [text])]));
  }
  return group;
}

/**
 * Asks for a Title and adds the Task: Active or Backlog (the list shown at
 * first), or, for a Subtask of `under`, only a Title, as it follows its
 * top-level Task's list.
 */
export function newTaskDialog(model: TasksModel, host: HTMLElement, under?: ListedTask): void {
  const title = textInput("");
  title.setAttribute("aria-label", "Title");
  title.placeholder = "Title";
  let planning = model.planning;
  const listChoice = under
    ? []
    : [
        choice(
          "List",
          lists.map(({ planning, title }) => ({ value: planning, text: title })),
          planning,
          (value) => (planning = value),
        ),
      ];
  openDialog(
    host,
    under ? "New Subtask" : "New Task",
    "Add",
    [title, ...listChoice],
    () => (under ? model.addSubtask(title.value, under) : model.add(title.value, planning)),
    () => title.value.trim() !== "",
  );
  title.focus();
}

/** Edits a Task's Title, Description, Priority, Tags and Blocked by; Save is off while the Title is empty. */
export function editTaskDialog(model: TasksModel, task: ListedTask, parent: HTMLElement): void {
  const title = textInput(task.title);
  const description = make("textarea", "tasks-input");
  description.rows = 4;
  description.value = task.description;
  let priority = task.priority;
  const priorities = [1, 2, 3, 4].map((value) => ({ value, text: `P${value}` }));
  const tags = new TagField(model, [...task.tags]);
  const blockers = new BlockedByField(model.blockerCandidates(task), [...task.blockedBy]);
  openDialog(
    parent,
    "Task",
    "Save",
    [
      field("Title", title),
      field("Description", description),
      choice("Priority", priorities, priority, (value) => (priority = value)),
      tags.element,
      blockers.element,
    ],
    () => {
      if (!model.setBlockedBy(task, blockers.ids())) return blockers.refuse();
      return model.edit(task, {
        title: title.value,
        description: description.value,
        priority,
        tags: tags.all(),
      });
    },
    () => title.value.trim() !== "",
  );
}

/**
 * The Tags of a Task: chips to remove, and an input that refuses forbidden
 * characters as they are typed (with a hint) and suggests the known Tags that
 * start with what is typed.
 */
class TagField {
  readonly element = make("fieldset", "tasks-tags");
  private readonly chips = make("ul", "tasks-chips");
  private readonly input = textInput("");
  private readonly hint = make("p", "tasks-hint", ["Tags use a–z, 0–9, - and / only."]);
  private readonly suggestions = make("ul", "tasks-suggestions");

  constructor(
    private readonly model: TasksModel,
    private readonly tags: string[],
  ) {
    this.input.setAttribute("aria-label", "Add Tag");
    this.input.placeholder = "Add Tag";
    this.input.autocapitalize = "off";
    this.input.autocomplete = "off";
    this.input.spellcheck = false;
    this.hint.hidden = true;
    this.input.addEventListener("input", () => {
      const allowed = TasksModel.allowedTagText(this.input.value);
      this.hint.hidden = allowed === this.input.value;
      this.input.value = allowed;
      this.showSuggestions();
    });
    // Enter adds the Tag; the form is saved with the button.
    this.input.addEventListener("keydown", (event) => {
      if (event.key !== "Enter" || this.input.value === "") return;
      event.preventDefault();
      event.stopPropagation();
      this.add(this.input.value);
    });
    this.element.append(
      make("legend", "", ["Tags"]),
      this.chips,
      this.input,
      this.hint,
      this.suggestions,
    );
    this.showChips();
  }

  /** The Tags chosen, with the one typed but not yet added. */
  all(): string[] {
    const typed = this.input.value;
    return typed && !this.tags.includes(typed) ? [...this.tags, typed] : this.tags;
  }

  private add(tag: string): void {
    if (!this.tags.includes(tag)) this.tags.push(tag);
    this.input.value = "";
    this.hint.hidden = true;
    this.showChips();
    this.showSuggestions();
  }

  private showChips(): void {
    this.chips.replaceChildren(
      ...this.tags.map((tag) => {
        const remove = button("tasks-chip", `${tag} ×`, () => {
          this.tags.splice(this.tags.indexOf(tag), 1);
          this.showChips();
        });
        remove.setAttribute("aria-label", `Remove ${tag}`);
        return make("li", "", [remove]);
      }),
    );
  }

  private showSuggestions(): void {
    const typed = this.input.value;
    const known = typed === "" ? [] : this.model.knownTags(typed);
    this.suggestions.replaceChildren(
      ...known
        .filter((tag) => !this.tags.includes(tag))
        .map((tag) => make("li", "", [button("tasks-suggestion", tag, () => this.add(tag))])),
    );
  }
}

/**
 * The Tasks a Task is Blocked by: chips to remove, and a menu of the Open
 * Tasks it could be Blocked by to add. Closed blockers stay listed, struck through.
 */
class BlockedByField {
  readonly element = make("fieldset", "tasks-blocked");
  private readonly chips = make("ul", "tasks-chips");
  private readonly choose = make("select", "tasks-input");
  private readonly hint = make("p", "tasks-hint", ["That would make Tasks wait on each other."]);

  constructor(
    private readonly candidates: ListedTask[],
    private readonly chosen: Blocker[],
  ) {
    this.choose.setAttribute("aria-label", "Add Blocked by");
    this.hint.hidden = true;
    this.choose.addEventListener("change", () => {
      const task = this.candidates.find((t) => t.id === this.choose.value);
      if (task) this.chosen.push({ id: task.id, title: task.title, closed: false });
      this.hint.hidden = true;
      this.show();
    });
    this.element.append(make("legend", "", ["Blocked by"]), this.chips, this.choose, this.hint);
    this.show();
  }

  ids(): string[] {
    return this.chosen.map((b) => b.id);
  }

  /** Shows why the choice was refused; false, for the save to return. */
  refuse(): false {
    this.hint.hidden = false;
    return false;
  }

  private show(): void {
    this.chips.replaceChildren(
      ...this.chosen.map((blocker) => {
        const remove = button("tasks-chip", `${blocker.title} ×`, () => {
          this.chosen.splice(this.chosen.indexOf(blocker), 1);
          this.show();
        });
        remove.setAttribute("aria-label", `Remove ${blocker.title}`);
        remove.classList.toggle("closed", blocker.closed);
        return make("li", "", [remove]);
      }),
    );
    const open = this.candidates.filter((t) => !this.chosen.some((b) => b.id === t.id));
    this.choose.replaceChildren(
      new Option("Add Blocked by…", ""),
      ...open.map((t) => new Option(t.title, t.id)),
    );
    this.choose.hidden = open.length === 0;
  }
}
