import { TaskQuickAdd, isValidTag } from "../core/Core";
import { button, make } from "../Dom";
import { QuickAddField } from "./QuickAddField";
import { SubtasksSection } from "./SubtasksSection";
import { tagLabel } from "./TagLabel";
import { type Blocker, type ListedTask, TasksModel } from "./TasksModel";

/** How the edit view moves between Tasks, each of which has an address of its own. */
export interface TaskPlaces {
  /** Shows the Task `id`, a Blocker or Subtask just pushed on the model's stack, at its address. */
  open(id: string): void;
  /** Returns to the Task before, at its address. */
  back(): void;
  /** Leaves the whole stack, back to the list's address. */
  close(): void;
}

let nextId = 0;

/** A modal `<dialog>` with a form of `fields` and `actions`, gone once closed. */
export function modal(
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
export function confirmButton(label: string): HTMLButtonElement {
  const confirm = make("button", "tasks-confirm", [label]);
  confirm.type = "submit";
  return confirm;
}

export function field(label: string, control: HTMLElement): HTMLLabelElement {
  return make("label", "tasks-field", [make("span", "", [label]), control]);
}

export function textInput(value: string): HTMLInputElement {
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
 * the Task with the Priority and Tags the Shortcuts give. Add or Enter
 * keeps the dialog open, cleared and focused for the next Task; Cancel or Esc
 * closes it. Add is off while the Title is empty.
 */
export function newTaskDialog(model: TasksModel, host: HTMLElement): void {
  let quickAdd = model.newQuickAdd();
  const title = new QuickAddField(model, quickAdd);
  const description = make("textarea", "tasks-input");
  description.rows = 4;
  description.setAttribute("aria-label", "Description");
  description.placeholder = "Description";
  const add = confirmButton("Add");
  const { dialog, form } = modal(
    host,
    "New Task",
    [title.element, description],
    [button("", "Cancel", () => dialog.close()), add],
  );
  const enableAdd = () => (add.disabled = title.title === "");
  form.addEventListener("input", enableAdd);
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    const extras = {
      description: description.value,
      priority: quickAdd.priority(),
      tags: quickAdd.tags(),
    };
    if (model.add(title.title, extras)) {
      quickAdd.free();
      quickAdd = model.newQuickAdd();
      title.use(quickAdd);
      description.value = "";
      enableAdd();
    }
    // Clicking Add moved the focus off the Title.
    title.focus();
  });
  dialog.addEventListener("close", () => quickAdd.free());
  enableAdd();
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
 * The Tags are one line of pills that opens a picker. Closing saves what is pending.
 * An Open Blocker's title, a Subtask row or the Parent opens it over this Task (the
 * Parent just below is gone back to instead); Back (with a Task
 * below) returns, and is off while the Title is empty. `refresh` redraws the Subtasks.
 */
export function editTaskDialog(
  model: TasksModel,
  task: ListedTask,
  parent: HTMLElement,
  places: Pick<TaskPlaces, "open" | "back">,
): { dialog: HTMLDialogElement; refresh: () => void } {
  let quickAdd = TaskQuickAdd.fromTitle(task.title);
  const tags = new TagField(model, [...task.tags], parent, () => save());
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
  const save = () => {
    clearTimeout(pause);
    model.setBlockedBy(task, blockers.ids());
    model.edit(task, {
      title: title.title,
      description: description.value,
      priority,
      tags: tags.chosen(),
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
  const openTask = (id: string) => {
    save();
    const depth = model.depth;
    if (!model.open(id)) return;
    // The model popped to the Task below (a Parent opened from its Subtask): the address follows.
    if (model.depth > depth) places.open(id);
    else places.back();
  };
  const subtasks = new SubtasksSection(model, task.id, openTask);
  const blockers = new BlockedByField(
    model.blockerCandidates(task),
    [...task.blockedBy],
    save,
    openTask,
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
      ...(task.parent === undefined
        ? []
        : [parentField(task.parent, task.parentTitle ?? "", openTask)]),
      subtasks.element,
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
    save();
    quickAdd.free();
    subtasks.stopAdding();
  });
  return { dialog, refresh: () => subtasks.refresh() };
}

/** A Subtask's Parent: one chip whose title opens it. */
function parentField(id: string, title: string, open: (id: string) => void): HTMLElement {
  const opener = button("tasks-chip-title", title, () => open(id));
  opener.setAttribute("aria-label", `Open ${title}`);
  const chips = make("ul", "tasks-chips", [make("li", "tasks-chip-pair", [opener])]);
  return make("fieldset", "tasks-blocked", [make("legend", "", ["Parent"]), chips]);
}

/**
 * The Tags of a Task: one line of pills in their Tag Colors, which opens the
 * picker when pressed. The pick is saved when the picker's Done is pressed.
 */
class TagField {
  readonly element = make("fieldset", "tasks-tags");
  private readonly row = button("tasks-tags-row", "", () => this.pick());

  constructor(
    private readonly model: TasksModel,
    private readonly tags: string[],
    private readonly parent: HTMLElement,
    private readonly changed: () => void,
  ) {
    this.element.append(make("legend", "", ["Tags"]), this.row);
    this.show();
  }

  /** The Tags chosen. */
  chosen(): string[] {
    return this.tags;
  }

  /** Shows exactly `tags`, e.g. after Quick Add added some. */
  set(tags: string[]): void {
    this.tags.splice(0, this.tags.length, ...tags);
    this.show();
  }

  private pick(): void {
    pickTagsDialog(this.parent, this.model, this.tags, (picked) => {
      this.set(picked);
      this.changed();
    });
  }

  private show(): void {
    const colors = new Map(this.model.allTags.map((t) => [t.name, t.color]));
    this.row.setAttribute("aria-label", `Tags: ${this.tags.join(", ") || "none"}`);
    this.row.replaceChildren(
      ...(this.tags.length === 0
        ? [make("span", "tasks-tags-empty", ["Add Tags"])]
        : this.tags.map((tag) => {
            const pill = tagLabel(tag, colors.get(tag));
            pill.classList.add("tasks-pill");
            return pill;
          })),
    );
  }
}

/**
 * A modal list of every known Tag with a checkbox each, and a search input at
 * the bottom that takes the focus on opening and also offers to create a Tag.
 * Done calls `done` with the Tags checked and closes; Cancel and Esc change nothing.
 */
function pickTagsDialog(
  parent: HTMLElement,
  model: TasksModel,
  chosen: string[],
  done: (tags: string[]) => void,
): void {
  const selected = [...chosen];
  const known = model.allTags;
  const colors = new Map(known.map((t) => [t.name, t.color]));
  const created: string[] = [];
  const dialog = make("dialog", "tasks-dialog tasks-picker");
  const heading = make("h2", "", ["Tags"]);
  dialog.setAttribute("aria-labelledby", (heading.id = `tasks-dialog-${nextId++}`));
  const confirm = button("tasks-confirm", "Done", () => {
    done(selected);
    dialog.close();
  });
  const list = make("ul", "tasks-picker-list");
  const hint = make("p", "tasks-hint", ["Tags use a–z, 0–9, - and / only."]);
  hint.hidden = true;
  const search = textInput("");
  search.type = "search";
  search.placeholder = search.ariaLabel = "Search or create";
  search.autocapitalize = "off";
  search.autocomplete = "off";
  search.spellcheck = false;
  // showModal focuses it, which opens the keyboard on touch devices.
  search.autofocus = true;

  const creatable = () =>
    isValidTag(search.value) && !colors.has(search.value) && !created.includes(search.value);
  const create = () => {
    created.push(search.value);
    selected.push(search.value);
    search.value = "";
    show();
  };
  const show = () => {
    const names = [...known.map((t) => t.name), ...created].filter((n) => n.includes(search.value));
    const rows = names.sort().map((tag) => {
      const box = make("input", "");
      box.type = "checkbox";
      box.checked = selected.includes(tag);
      box.addEventListener("change", () => {
        if (box.checked) selected.push(tag);
        else selected.splice(selected.indexOf(tag), 1);
      });
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
  dialog.append(make("div", "tasks-picker-bar", [heading, confirm]), list, hint, search);
  dialog.addEventListener("close", () => dialog.remove());
  show();
  parent.append(dialog);
  dialog.showModal();
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
