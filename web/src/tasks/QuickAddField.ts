import { type Inline, type TaskQuickAdd, toggleInline } from "../core/Core";
import { button, make } from "../Dom";
import { tagLabel } from "./TagLabel";
import { type Range, drawRanges, formatKey, formatRanges } from "./TitleText";
import type { TasksModel } from "./TasksModel";
import { caretOffset, placeCaret, selectRange, selectionOffsets, textEdit } from "./TextCaret";

let nextId = 0;

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

/**
 * The Title field of a Task: Quick Add's text, wrapping onto more lines as it
 * grows, with its Shortcuts highlighted, and the Tags to suggest below it while the cursor is in an `@`
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
    // The email keyboard has "@" beside the space bar for Tags, without the `#` Quick Add doesn't use.
    editor.inputMode = "email";
    editor.autocapitalize = "sentences";
    editor.setAttribute("autocorrect", "on");
    editor.spellcheck = true;
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
    const format = formatKey(event);
    if (format !== undefined) {
      event.preventDefault();
      this.format(format);
      return;
    }
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

  /** Toggles Inline formatting on the selection, which stays selected. */
  private format(kind: Inline): void {
    const range = selectionOffsets(this.editor);
    if (!range) return;
    const edit = toggleInline(this.quickAdd.text(), range.start, range.end, kind);
    this.quickAdd.replace(edit.start, edit.end, edit.replacement);
    this.quickAdd.moveCursor(edit.selectionEnd);
    this.draw();
    selectRange(this.editor, edit.selectionStart, edit.selectionEnd);
    this.changed();
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
      const ranges: Range[] = formatRanges(text);
      const labels: HTMLElement[] = [];
      for (const { start, end, meaning } of highlights) {
        const word = text.slice(start, end);
        const label = `${word}, ${meaning}`;
        const mark = make("mark", "tasks-shortcut");
        mark.dataset.start = String(start);
        mark.setAttribute("aria-label", label);
        ranges.push({ start, end, element: mark });
        labels.push(make("li", "", [this.ignoreButton(label, start)]));
      }
      this.editor.replaceChildren(...drawRanges(text, ranges));
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
