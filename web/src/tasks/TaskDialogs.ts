import { Planning, TaskQuickAdd, isValidTag } from "../core/Core";
import { button, make } from "../Dom";
import { QuickAddField } from "./QuickAddField";
import { tagLabel } from "./TagLabel";
import { type Blocker, type ListedTask, TasksModel } from "./TasksModel";

/** How the edit view moves between Tasks, each of which has an address of its own. */
export interface TaskPlaces {
  /** Shows the Task `id`, a Blocker just pushed on the model's stack, at its address. */
  open(id: string): void;
  /** Returns to the Task before, at its address. */
  back(): void;
  /** Leaves the whole stack, back to the list's address. */
  close(): void;
}

let nextId = 0;

/** A modal `<dialog>` with a form of `fields` and `actions`, gone once closed. */
function modal(
  parent: HTMLElement,
  title: string,
  fields: HTMLElement[],
  actions: HTMLElement[],
): { dialog: HTMLDialogElement; form: HTMLFormElement } {
  const dialog = make("dialog", "tasks-dialog");
  const heading = make("h2", "", [title]);
  dialog.setAttribute("aria-labelledby", (heading.id = `tasks-dialog-${nextId++}`));
  const form = make("form", "tasks-form");
  form.append(heading, ...fields, make("div", "tasks-actions", actions));
  dialog.append(form);
  dialog.addEventListener("close", () => dialog.remove());
  parent.append(dialog);
  dialog.showModal();
  return { dialog, form };
}

/** A confirm button of a form, off while it can't be saved. */
function confirmButton(label: string): HTMLButtonElement {
  const confirm = make("button", "tasks-confirm", [label]);
  confirm.type = "submit";
  return confirm;
}

/** A modal form saved with its confirm button or Enter, closed once `save` succeeds, or with Cancel. */
function openDialog(
  parent: HTMLElement,
  title: string,
  confirmLabel: string,
  fields: HTMLElement[],
  save: () => boolean,
  canSave: () => boolean,
  closed: () => void = () => {},
): void {
  const confirm = confirmButton(confirmLabel);
  const { dialog, form } = modal(parent, title, fields, [
    button("", "Cancel", () => dialog.close()),
    confirm,
  ]);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (save()) dialog.close();
  });
  dialog.addEventListener("close", closed);
  form.addEventListener("input", () => (confirm.disabled = !canSave()));
  confirm.disabled = !canSave();
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
    input.value = String(value);
    input.checked = value === chosen;
    input.addEventListener("change", () => change(value));
    group.append(make("label", "", [input, make("span", "", [text])]));
  }
  return group;
}

/** The Parent of `task`: one of `candidates` or None; `change` gets the pick as soon as it is made. */
function parentSelect(
  task: ListedTask,
  candidates: ListedTask[],
  change: (parent: ListedTask | undefined) => void,
): HTMLLabelElement {
  const select = make("select", "tasks-input");
  select.append(new Option("None", ""), ...candidates.map((t) => new Option(t.title, t.id)));
  select.value = task.parent ?? "";
  select.addEventListener("change", () => change(candidates.find((t) => t.id === select.value)));
  return field("Parent", select);
}

/** Asks to Delete a Tag, which is Removed from every Item, then calls `deleted`; Cancel changes nothing. */
export function deleteTagDialog(
  model: TasksModel,
  host: HTMLElement,
  tag: string,
  deleted: () => void,
): void {
  const confirm = confirmButton("Delete");
  const { dialog, form } = modal(
    host,
    `Delete '${tag}'? It is Removed from every Item.`,
    [],
    [button("", "Cancel", () => dialog.close()), confirm],
  );
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    model.deleteTag(tag);
    dialog.close();
    deleted();
  });
}

/**
 * Asks for a Title, typed with Quick Add Shortcuts, and a Description and adds
 * the Task, Active, with the Priority and Tags the Shortcuts give; a Subtask of
 * `under` follows its top-level Task's list.
 */
export function newTaskDialog(model: TasksModel, host: HTMLElement, under?: ListedTask): void {
  const quickAdd = new TaskQuickAdd();
  const title = new QuickAddField(model, quickAdd);
  const description = make("textarea", "tasks-input");
  description.rows = 4;
  description.setAttribute("aria-label", "Description");
  description.placeholder = "Description";
  openDialog(
    host,
    under ? "New Subtask" : "New Task",
    "Add",
    [title.element, description],
    () => {
      const extras = {
        description: description.value,
        priority: quickAdd.priority(),
        tags: quickAdd.tags(),
      };
      return under
        ? model.addSubtask(title.title, under, extras)
        : model.add(title.title, Planning.Active, extras);
    },
    () => title.title !== "",
    () => quickAdd.free(),
  );
  title.focus();
}

/** How long typing must pause before the text is saved. */
const typingPause = 500;

/**
 * Edits a Task's Title, Description, Priority, Tags and Blocked by, saving as
 * the user goes: text after a pause and when its field loses focus, the rest
 * at once. The Title is Quick Add, starting with everything Ignored: the pause
 * saves it without pending Shortcuts, and Return, losing focus and closing
 * apply them (Tags are added, a Priority Shortcut replaces the Priority); the
 * Priority control Ignores a pending Priority Shortcut. Done closes it. While
 * the Title is empty the last saved one is kept, a hint says why, and the
 * dialog can't be left (Done is off, Esc ignored).
 * Closing saves what is pending, with a Tag typed but not added if it is valid.
 * An Open Blocker's title opens it over this Task; Back (with a Task below) returns,
 * and is off while the Title is empty.
 */
export function editTaskDialog(
  model: TasksModel,
  task: ListedTask,
  parent: HTMLElement,
  places: Pick<TaskPlaces, "open" | "back">,
): HTMLDialogElement {
  let quickAdd = TaskQuickAdd.fromTitle(task.title);
  const tags = new TagField(model, [...task.tags], () => save());
  const title = new QuickAddField(model, quickAdd, {
    taskTags: () => tags.chosen(),
    submit: () => commit(),
  });
  const titleHint = make("p", "tasks-hint", ["A Task needs a Title."]);
  const description = make("textarea", "tasks-input");
  description.rows = 4;
  description.value = task.description;
  let priority = task.priority;
  let pause: ReturnType<typeof setTimeout> | undefined;
  const save = (withTypedTag = false) => {
    clearTimeout(pause);
    model.setBlockedBy(task, blockers.ids());
    model.edit(task, {
      title: title.title,
      description: description.value,
      priority,
      tags: withTypedTag ? tags.all() : tags.chosen(),
    });
  };
  /** Applies the Shortcuts of the Title: they leave the field, the Priority control and the Tags show the result. With none pending it just saves. */
  const commit = () => {
    const pending = quickAdd.priority() !== undefined || quickAdd.tags().length > 0;
    const edit = { description: description.value, priority, tags: tags.chosen() };
    const saved = pending ? model.commitQuickAdd(quickAdd, task, edit) : undefined;
    if (!saved) return save();
    clearTimeout(pause);
    priority = saved.priority;
    showPriority();
    tags.set(saved.tags);
    quickAdd.free();
    quickAdd = TaskQuickAdd.fromTitle(saved.title);
    title.use(quickAdd);
  };
  const openBlocker = (id: string) => {
    save();
    if (model.openBlocker(id)) places.open(id);
  };
  const blockers = new BlockedByField(
    model.blockerCandidates(task),
    [...task.blockedBy],
    save,
    openBlocker,
  );
  const parentChoice = parentSelect(task, model.parentCandidates(task), (parent) =>
    model.setParent(task, parent),
  );
  const back = button("tasks-back", "Back", () => places.back());
  back.hidden = model.depth < 2;
  const priorities = [1, 2, 3, 4].map((value) => ({ value, text: `P${value}` }));
  const done = confirmButton("Done");
  const titleEmpty = () => title.title === "";
  const priorityChoice = choice("Priority", priorities, priority, (value) => {
    priority = value;
    quickAdd.ignorePriority(); // the last action wins
    title.redraw();
    save();
  });
  const showPriority = () => {
    priorityChoice.querySelector<HTMLInputElement>(`input[value="${priority}"]`)!.checked = true;
  };
  const { dialog, form } = modal(
    parent,
    "Task",
    [
      back,
      title.element,
      titleHint,
      field("Description", description),
      priorityChoice,
      tags.element,
      parentChoice,
      blockers.element,
    ],
    [done],
  );
  const isText = (target: EventTarget | null) =>
    target === description || title.element.contains(target as Node);
  const showTitleState = () => {
    titleHint.hidden = !titleEmpty();
    done.disabled = titleEmpty();
    back.disabled = titleEmpty();
  };
  showTitleState();
  form.addEventListener("input", (event) => {
    if (isText(event.target)) {
      clearTimeout(pause);
      pause = setTimeout(save, typingPause);
    }
    // The model holds the stack still as soon as the Title is empty, not after the pause.
    if (titleEmpty() !== model.isLeavingBlocked) save();
    showTitleState();
  });
  form.addEventListener("focusout", (event) => {
    if (!dialog.open) return; // closing has saved, and freed Quick Add
    const leaves = (field: HTMLElement) =>
      field.contains(event.target as Node) && !field.contains(event.relatedTarget as Node);
    if (leaves(title.element)) commit();
    else if (event.target === description) save();
  });
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (!titleEmpty()) dialog.close();
  });
  dialog.addEventListener("cancel", (event) => {
    if (titleEmpty()) event.preventDefault();
  });
  dialog.addEventListener("close", () => {
    commit();
    save(true);
    quickAdd.free();
  });
  return dialog;
}

/**
 * The Tags of a Task: chips to remove, and an input that refuses forbidden
 * characters as they are typed (with a hint) and suggests the known Tags that
 * start with what is typed. Text that is no Tag yet (`work/`) is not added.
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
    private readonly changed: () => void,
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
      if (isValidTag(this.input.value)) this.add(this.input.value);
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

  /** The Tags added so far. */
  chosen(): string[] {
    return this.tags;
  }

  /** Shows exactly `tags`, e.g. after Quick Add added some. */
  set(tags: string[]): void {
    this.tags.splice(0, this.tags.length, ...tags);
    this.showChips();
  }

  /** The Tags chosen, with the one typed but not yet added, if it is one. */
  all(): string[] {
    const typed = this.input.value;
    return isValidTag(typed) && !this.tags.includes(typed) ? [...this.tags, typed] : this.tags;
  }

  private add(tag: string): void {
    if (!this.tags.includes(tag)) this.tags.push(tag);
    this.input.value = "";
    this.hint.hidden = true;
    this.showChips();
    this.showSuggestions();
    this.changed();
  }

  private showChips(): void {
    const colors = new Map(this.model.allTags.map((t) => [t.name, t.color]));
    this.chips.replaceChildren(
      ...this.tags.map((tag) => {
        const remove = button("tasks-chip", "", () => {
          this.tags.splice(this.tags.indexOf(tag), 1);
          this.showChips();
          this.changed();
        });
        remove.append(tagLabel(tag, colors.get(tag)), " ×");
        remove.setAttribute("aria-label", `Remove ${tag}`);
        return make("li", "", [remove]);
      }),
    );
  }

  private showSuggestions(): void {
    const typed = this.input.value;
    const { matches, create } = this.model.suggestTags(typed, this.tags);
    const rows = matches.map((tag) => button("tasks-suggestion", tag, () => this.add(tag)));
    if (create) {
      rows.push(button("tasks-suggestion", `Create Tag '${typed}'`, () => this.add(typed)));
    }
    this.suggestions.replaceChildren(...rows.map((row) => make("li", "", [row])));
  }
}

/**
 * The Tasks a Task is Blocked by: chips with a title button to open an Open
 * Blocker and a × to remove it, and a menu of the Open Tasks it could be
 * Blocked by to add. Closed blockers stay listed, struck through, with only the ×.
 */
class BlockedByField {
  readonly element = make("fieldset", "tasks-blocked");
  private readonly chips = make("ul", "tasks-chips");
  private readonly choose = make("select", "tasks-input");

  constructor(
    private readonly candidates: ListedTask[],
    private readonly chosen: Blocker[],
    private readonly changed: () => void,
    private readonly open: (id: string) => void,
  ) {
    this.choose.setAttribute("aria-label", "Add Blocked by");
    this.choose.addEventListener("change", () => {
      const task = this.candidates.find((t) => t.id === this.choose.value);
      if (task) this.chosen.push({ id: task.id, title: task.title, closed: false });
      this.show();
      this.changed();
    });
    this.element.append(make("legend", "", ["Blocked by"]), this.chips, this.choose);
    this.show();
  }

  ids(): string[] {
    return this.chosen.map((b) => b.id);
  }

  private show(): void {
    this.chips.replaceChildren(
      ...this.chosen.map((blocker) => {
        const remove = button("tasks-chip", "×", () => {
          this.chosen.splice(this.chosen.indexOf(blocker), 1);
          this.show();
          this.changed();
        });
        remove.setAttribute("aria-label", `Remove ${blocker.title}`);
        const title = make(blocker.closed ? "span" : "button", "tasks-chip-title", [blocker.title]);
        if (title instanceof HTMLButtonElement) {
          title.type = "button";
          title.setAttribute("aria-label", `Open ${blocker.title}`);
          title.addEventListener("click", () => this.open(blocker.id));
        }
        const chip = make("li", "tasks-chip-pair", [title, remove]);
        chip.classList.toggle("closed", blocker.closed);
        return chip;
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
