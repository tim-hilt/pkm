import type { TaskQuickAdd } from "../core/Core";
import { button, make } from "../Dom";
import { tagLabel } from "./TagLabel";
import type { TasksModel } from "./TasksModel";

let nextId = 0;

/**
 * The UTF-16 range of `old` that `now` replaced, and what replaced it: all but
 * their common start and end, never splitting a surrogate pair.
 */
export function textEdit(
  old: string,
  now: string,
): { start: number; end: number; inserted: string } {
  const limit = Math.min(old.length, now.length);
  let head = 0;
  while (head < limit && old[head] === now[head]) head++;
  if (head > 0 && (isLowSurrogate(old, head) || isLowSurrogate(now, head))) head--;
  let tail = 0;
  while (tail < limit - head && old[old.length - 1 - tail] === now[now.length - 1 - tail]) tail++;
  if (tail > 0 && isLowSurrogate(old, old.length - tail)) tail--;
  return { start: head, end: old.length - tail, inserted: now.slice(head, now.length - tail) };
}

function isLowSurrogate(text: string, at: number): boolean {
  const unit = text.charCodeAt(at);
  return unit >= 0xdc00 && unit <= 0xdfff;
}

/** A Shortcut to draw: plain copy of the core's, which must be freed. */
interface Highlight {
  start: number;
  end: number;
  /** What it sets, for screen readers: "Priority 1" or "Tag bug". */
  meaning: string;
}

/** What to suggest while the cursor is in an `@` word. */
interface Query {
  query: string;
  leaveOut: string[];
}

/** The UTF-16 offset of the caret in `root`'s text; none if it is elsewhere or a range is selected. */
function caretOffset(root: HTMLElement): number | undefined {
  const selection = getSelection();
  if (!selection?.isCollapsed || !selection.focusNode || !root.contains(selection.focusNode))
    return undefined;
  const range = document.createRange();
  range.selectNodeContents(root);
  range.setEnd(selection.focusNode, selection.focusOffset);
  return range.toString().length;
}

/** Puts the caret at the UTF-16 `offset` of `root`'s text, at the end of a text node rather than the start of the next. */
function placeCaret(root: HTMLElement, offset: number): void {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let seen = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const length = node.textContent?.length ?? 0;
    if (seen + length >= offset) {
      range.setStart(node, offset - seen);
      range.collapse(true);
      break;
    }
    seen += length;
  }
  getSelection()?.removeAllRanges();
  getSelection()?.addRange(range);
}

/**
 * The Title field of a Task: Quick Add's text on one line, with its Shortcuts
 * highlighted, and the Tags to suggest below it while the cursor is in an `@`
 * word. It only draws and forwards what is typed, the cursor, Return, picks and
 * Ignores; every rule lives in the core's `TaskQuickAdd`, which the caller
 * owns and frees. It can start from a saved
 * Title (`TaskQuickAdd.fromTitle`) to edit one.
 *
 * The browser edits the text; each change is handed to Quick Add as the range
 * it replaced, and the field is drawn again from Quick Add's text, with the
 * caret kept. A mouse press on a highlight Ignores it; each Shortcut is also
 * labelled with its meaning and has an Ignore button for screen readers.
 */
export class QuickAddField {
  readonly element = make("div", "tasks-quick-add");
  private readonly editor = make("div", "tasks-input tasks-quick-input");
  private readonly list = make("ul", "tasks-suggestions");
  private readonly ignores = make("ul", "tasks-ignores");
  /** Text and Shortcuts as last drawn, to draw only what changed. */
  private drawn = "";
  private query: Query | undefined;
  private queryKey = "";
  /** The suggestion the arrow keys are on. */
  private selected = 0;
  /** Escape closed the list for this query. */
  private closed = false;
  private composing = false;
  private suggestions: { tag: string; isNew: boolean }[] = [];
  /** The rows as last drawn, to draw only what changed. */
  private listKey = "";

  private readonly taskTags: () => readonly string[];
  private readonly submit: () => void;

  /**
   * `taskTags` gives the Tags the Task has now, which the list doesn't suggest;
   * `submit` is what Return does while the list is closed, submitting the form
   * if omitted; `blur` is called when the field loses focus.
   */
  constructor(
    private readonly model: TasksModel,
    private quickAdd: TaskQuickAdd,
    options: {
      taskTags?: () => readonly string[];
      submit?: () => void;
      blur?: () => void;
    } = {},
  ) {
    this.taskTags = options.taskTags ?? (() => []);
    this.submit = options.submit ?? (() => this.element.closest("form")?.requestSubmit());
    const editor = this.editor;
    editor.setAttribute("contenteditable", "plaintext-only");
    editor.setAttribute("role", "combobox");
    editor.setAttribute("aria-label", "Title");
    editor.setAttribute("aria-autocomplete", "list");
    editor.dataset.placeholder = "Title";
    editor.spellcheck = false;
    editor.autocapitalize = "off";
    this.list.id = `tasks-quick-suggestions-${nextId++}`;
    this.list.setAttribute("role", "listbox");
    editor.setAttribute("aria-controls", this.list.id);
    this.element.append(editor, this.list, this.ignores);
    editor.addEventListener("input", (event) => {
      if (!(event as InputEvent).isComposing) this.sync();
    });
    editor.addEventListener("compositionstart", () => (this.composing = true));
    editor.addEventListener("compositionend", () => {
      this.composing = false;
      this.sync();
    });
    editor.addEventListener("keydown", (event) => this.keydown(event));
    editor.addEventListener("beforeinput", (event) => {
      // A line break typed (not pasted: Quick Add turns those into spaces).
      if (event.inputType === "insertParagraph" || event.inputType === "insertLineBreak") {
        event.preventDefault();
      }
    });
    editor.addEventListener("mousedown", (event) => this.press(event));
    editor.addEventListener("focus", () =>
      document.addEventListener("selectionchange", this.moved),
    );
    editor.addEventListener("blur", () => {
      document.removeEventListener("selectionchange", this.moved);
      this.drawSuggestions(false);
      options.blur?.();
    });
    this.draw();
  }

  /** Quick Add's Title (the text without Shortcuts). */
  get title(): string {
    return this.quickAdd.title();
  }

  /** Shows another Quick Add (which the caller owns, as the first) instead, e.g. after a commit. */
  use(quickAdd: TaskQuickAdd): void {
    this.quickAdd = quickAdd;
    this.redraw();
  }

  /** Draws again after Quick Add was changed from outside, e.g. its Priority Ignored. */
  redraw(): void {
    this.draw();
  }

  /** Empties the field, which keeps the focus. */
  clear(): void {
    this.quickAdd.replace(0, this.quickAdd.text().length, "");
    this.draw();
  }

  /** Takes the keyboard focus. */
  focus(): void {
    this.editor.focus();
  }

  private readonly moved = (): void => {
    if (!this.composing) this.sync();
  };

  /** Hands what changed in the editor and the caret to Quick Add, and draws what differs. */
  private sync(): void {
    const old = this.quickAdd.text();
    const now = this.editor.textContent ?? "";
    if (now !== old) {
      const { start, end, inserted } = textEdit(old, now);
      this.quickAdd.replace(start, end, inserted);
    }
    // While a range is selected the cursor, and so the highlights, stay: a
    // redraw would drop the selection. Line breaks pasted in become spaces,
    // which keeps the length.
    const caret = caretOffset(this.editor);
    if (caret !== undefined && caret <= this.quickAdd.text().length)
      this.quickAdd.moveCursor(caret);
    this.draw();
  }

  /** A press on a highlight Ignores that Shortcut, keeping the caret and focus where they are. */
  private press(event: MouseEvent): void {
    const hit = (event.target as Element).closest<HTMLElement>("mark[data-start]");
    if (!hit) return;
    event.preventDefault();
    this.ignore(Number(hit.dataset.start));
  }

  private ignore(at: number): void {
    this.quickAdd.ignore(at);
    this.draw();
    this.changed();
  }

  private keydown(event: KeyboardEvent): void {
    if (event.isComposing) return;
    const open = this.suggestions.length > 0;
    const count = this.suggestions.length;
    switch (event.key) {
      case "Enter":
        event.preventDefault();
        // While the list is open Return picks instead of adding.
        if (open) this.pick(this.suggestions[this.selected].tag);
        else this.submit();
        break;
      case "Tab":
        if (!open || event.shiftKey) return;
        event.preventDefault();
        this.pick(this.suggestions[this.selected].tag);
        break;
      case "ArrowDown":
      case "ArrowUp":
        if (!open) return;
        event.preventDefault();
        this.selected = (this.selected + (event.key === "ArrowDown" ? 1 : -1) + count) % count;
        this.drawSuggestions(true);
        break;
      case "Escape":
        if (!open) return;
        // Closes the list, not the dialog.
        event.preventDefault();
        event.stopPropagation();
        this.closed = true;
        this.drawSuggestions(true);
        break;
    }
  }

  /** Completes the `@` word to `@tag `. */
  private pick(tag: string): void {
    this.quickAdd.pickTag(tag);
    this.draw();
    this.editor.focus();
    this.changed();
  }

  /** Tells the form that the text changed other than by typing, so that it can check what it can save. */
  private changed(): void {
    this.editor.dispatchEvent(new Event("input", { bubbles: true }));
  }

  private highlights(): Highlight[] {
    return this.quickAdd.highlights().map((shortcut) => {
      const plain = {
        start: shortcut.start,
        end: shortcut.end,
        meaning:
          shortcut.priority !== undefined ? `Priority ${shortcut.priority}` : `Tag ${shortcut.tag}`,
      };
      shortcut.free();
      return plain;
    });
  }

  /** Draws Quick Add's text with its highlights, the caret where Quick Add has it, and the suggestions. */
  private draw(): void {
    const text = this.quickAdd.text();
    const highlights = this.highlights();
    const key = JSON.stringify([text, highlights]);
    if (key !== this.drawn) {
      this.drawn = key;
      const nodes: (Node | string)[] = [];
      const labels: HTMLElement[] = [];
      let at = 0;
      for (const { start, end, meaning } of highlights) {
        const word = text.slice(start, end);
        const label = `${word}, ${meaning}`;
        const mark = make("mark", "tasks-shortcut", [word]);
        mark.dataset.start = String(start);
        mark.setAttribute("aria-label", label);
        nodes.push(text.slice(at, start), mark);
        labels.push(make("li", "", [this.ignoreButton(label, start)]));
        at = end;
      }
      nodes.push(text.slice(at));
      this.editor.replaceChildren(...nodes.filter((node) => node !== ""));
      this.ignores.replaceChildren(...labels);
      if (document.activeElement === this.editor) placeCaret(this.editor, this.quickAdd.cursor());
    }
    this.drawSuggestions(document.activeElement === this.editor);
  }

  private ignoreButton(label: string, start: number): HTMLButtonElement {
    const ignore = button("", `Ignore ${label}`, () => this.ignore(start));
    ignore.tabIndex = -1;
    return ignore;
  }

  /** The list: matching Tags, then Create for a new valid one; empty while the field has no focus or Escape closed it. */
  private drawSuggestions(focused: boolean): void {
    const found = this.quickAdd.tagQuery();
    this.query = found && { query: found.query, leaveOut: found.leaveOut };
    found?.free();
    const key = JSON.stringify(this.query ?? null);
    if (key !== this.queryKey) {
      this.queryKey = key;
      this.selected = 0;
      this.closed = false;
    }
    this.suggestions = [];
    if (this.query && focused && !this.closed) {
      const { query, leaveOut } = this.query;
      const { matches, create } = this.model.suggestTags(query, [...leaveOut, ...this.taskTags()]);
      this.suggestions = matches.map((tag) => ({ tag, isNew: false }));
      if (create) this.suggestions.push({ tag: query, isNew: true });
    }
    const listKey = JSON.stringify([this.suggestions, this.selected]);
    if (listKey !== this.listKey) {
      this.listKey = listKey;
      this.drawList();
    }
    this.editor.setAttribute("aria-expanded", String(this.suggestions.length > 0));
    if (this.suggestions.length > 0) {
      this.editor.setAttribute("aria-activedescendant", `${this.list.id}-${this.selected}`);
    } else {
      this.editor.removeAttribute("aria-activedescendant");
    }
  }

  private drawList(): void {
    const colors = new Map(this.model.allTags.map((t) => [t.name, t.color]));
    this.list.replaceChildren(
      ...this.suggestions.map(({ tag, isNew }, index) => {
        const row = make("li", "tasks-suggestion", [
          isNew ? `Create Tag '${tag}'` : tagLabel(tag, colors.get(tag)),
        ]);
        row.id = `${this.list.id}-${index}`;
        row.setAttribute("role", "option");
        row.setAttribute("aria-selected", String(index === this.selected));
        row.classList.toggle("selected", index === this.selected);
        // Keeps the focus in the field, whose `@` word the pick completes.
        row.addEventListener("mousedown", (event) => event.preventDefault());
        row.addEventListener("click", () => this.pick(tag));
        return row;
      }),
    );
  }
}
