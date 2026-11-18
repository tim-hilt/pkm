import { insertImage, rawRange, returnAt, shiftItems, toggleInline } from "../core/Core";
import { make } from "../Dom";
import type { DescriptionImages } from "./DescriptionImages";
import { type Raw, descriptionNodes } from "./DescriptionText";
import { formatKey } from "./TitleText";
import { caretOffset, placeCaret, selectRange, selectionOffsets } from "./TextCaret";

/**
 * The Description of a Task, edited in Live Preview: its model is the Markdown
 * string, which is all the element holds. The browser edits the text; after each
 * edit, and when the caret moves to another line, the field is drawn again from
 * the core's parse, with the caret kept. Every line is rendered but the one
 * with the caret (the whole fence, in a code block), which shows its markers.
 * Rendered links open on a click; pasting inserts plain text. A picture pasted
 * or dropped is added through `images` and put on a line of its own as an Image;
 * without `images` pictures are ignored.
 */
export class DescriptionField {
  /** The `contenteditable` element, which sends `input` events like a text field. */
  readonly element = make("div", "tasks-input tasks-description");
  /** Text and raw range as last drawn, to draw only what changed. */
  private drawn: string | undefined;
  private composing = false;
  /** The texts and carets to undo and redo through, and where in them the field is. */
  private history: { text: string; caret: number | undefined }[] = [];
  private at = -1;

  constructor(
    text = "",
    private readonly images?: DescriptionImages,
  ) {
    const editor = this.element;
    editor.setAttribute("contenteditable", "plaintext-only");
    editor.setAttribute("role", "textbox");
    editor.setAttribute("aria-multiline", "true");
    editor.setAttribute("aria-label", "Description");
    editor.dataset.placeholder = "Description";
    editor.autocapitalize = "sentences";
    editor.spellcheck = true;
    this.value = text;
    editor.addEventListener("input", (event) => {
      if (!(event as InputEvent).isComposing) {
        this.draw();
        this.record();
      }
    });
    editor.addEventListener("compositionstart", () => (this.composing = true));
    editor.addEventListener("compositionend", () => {
      this.composing = false;
      this.draw();
      this.record();
    });
    editor.addEventListener("beforeinput", (event) => this.beforeInput(event));
    editor.addEventListener("keydown", (event) => this.keyDown(event));
    editor.addEventListener("mousedown", (event) => this.open(event));
    editor.addEventListener("paste", (event) => this.take(event, event.clipboardData));
    editor.addEventListener("dragover", (event) => {
      if (this.images && hasFiles(event.dataTransfer)) event.preventDefault();
    });
    editor.addEventListener("drop", (event) =>
      this.take(event, event.dataTransfer, offsetAt(event.clientX, event.clientY, this.element)),
    );
    const moved = () => {
      if (!this.composing) this.draw();
    };
    editor.addEventListener("focus", () => document.addEventListener("selectionchange", moved));
    editor.addEventListener("blur", () => {
      document.removeEventListener("selectionchange", moved);
      this.draw();
    });
  }

  /** The Markdown text. */
  get value(): string {
    return this.element.textContent ?? "";
  }

  set value(text: string) {
    this.drawn = undefined;
    this.draw(text);
    this.history = [{ text, caret: undefined }];
    this.at = 0;
  }

  /** Takes the keyboard focus. */
  focus(): void {
    this.element.focus();
  }

  /** Adds the pictures `data` carries, at `at` (the selection, if none), instead of the browser's own paste or drop; text and anything else is left to it. */
  private take(event: Event, data: DataTransfer | null, at?: number): void {
    const pictures = [...(data?.files ?? [])].filter((file) => file.type.startsWith("image/"));
    if (!this.images || pictures.length === 0) return;
    event.preventDefault();
    const end = this.value.length;
    const selected = at === undefined ? selectionOffsets(this.element) : { start: at, end: at };
    void this.insertPictures(pictures, selected ?? { start: end, end });
  }

  private async insertPictures(files: File[], selected: { start: number; end: number }) {
    let { start, end } = selected;
    for (const file of files) {
      const reference = await this.images?.add(new Uint8Array(await file.arrayBuffer()));
      if (reference === undefined) continue;
      const edit = insertImage(this.value, start, end, reference);
      this.apply(edit);
      start = end = edit.selectionEnd;
    }
  }

  /** Undo and redo walk our history. Return continues or ends a list item, else breaks the line: the browser's own leaves a second break after the last line. */
  private beforeInput(event: InputEvent): void {
    const undo = event.inputType === "historyUndo";
    if (undo || event.inputType === "historyRedo") {
      event.preventDefault();
      this.travel(undo ? -1 : 1);
      return;
    }
    if (event.inputType !== "insertParagraph" && event.inputType !== "insertLineBreak") return;
    event.preventDefault();
    const selected = selectionOffsets(this.element);
    if (selected) this.apply(returnAt(this.value, selected.start, selected.end));
  }

  /** Mod-Z and Mod-Shift-Z undo and redo; Mod-B, Mod-I and Mod-E toggle Inline formatting; Tab and Shift-Tab indent and outdent the list items, and elsewhere move the focus on. */
  private keyDown(event: KeyboardEvent): void {
    if ((event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === "z") {
      event.preventDefault();
      this.travel(event.shiftKey ? 1 : -1);
      return;
    }
    const selected = selectionOffsets(this.element);
    if (!selected) return;
    const format = formatKey(event);
    if (format !== undefined) {
      event.preventDefault();
      this.apply(toggleInline(this.value, selected.start, selected.end, format));
      return;
    }
    if (event.key !== "Tab" || event.altKey || event.ctrlKey || event.metaKey) return;
    const edit = shiftItems(this.value, selected.start, selected.end, event.shiftKey);
    if (!edit) return;
    event.preventDefault();
    this.apply(edit);
  }

  /** Applies a core edit to the text and selects what it says. */
  private apply(edit: ReturnType<typeof returnAt>): void {
    const text = this.value;
    this.draw(
      text.slice(0, edit.start) + edit.replacement + text.slice(edit.end),
      edit.selectionEnd,
    );
    selectRange(this.element, edit.selectionStart, edit.selectionEnd);
    this.element.dispatchEvent(new Event("input", { bubbles: true }));
  }

  /** Remembers the text and caret as a state to undo to. */
  private record(): void {
    const top = this.history[this.at];
    if (top?.text === this.value) return;
    this.history.splice(this.at + 1, Infinity, {
      text: this.value,
      caret: caretOffset(this.element),
    });
    this.at++;
  }

  /** Steps through the history, as the browser's own undo can't follow a field that is drawn again. */
  private travel(step: -1 | 1): void {
    const state = this.history[this.at + step];
    if (!state) return;
    this.at += step;
    this.draw(state.text, state.caret ?? state.text.length);
    this.element.dispatchEvent(new Event("input", { bubbles: true }));
  }

  /** Draws `text` (the element's, unless a change is handed in) again if it or what is raw changed, with the caret kept or put at `moveTo`. */
  private draw(text = this.value, moveTo?: number): void {
    const editing = document.activeElement === this.element;
    const caret = moveTo ?? (editing ? caretOffset(this.element) : undefined);
    // A selected range keeps what is drawn: drawing again would drop it.
    if (editing && caret === undefined && this.drawn !== undefined) return;
    const raw: Raw | undefined = caret === undefined ? undefined : rawRange(text, caret);
    const key = JSON.stringify([text, raw && [raw.start, raw.end]]);
    if (key === this.drawn) return;
    this.drawn = key;
    const nodes = descriptionNodes(text, raw, this.images);
    // A last line break shows only with something after it.
    if (text.endsWith("\n")) nodes.push(document.createElement("br"));
    this.element.replaceChildren(...nodes);
    if (caret !== undefined) placeCaret(this.element, caret);
  }

  private open(event: MouseEvent): void {
    const href = (event.target as Element).closest<HTMLElement>("a[data-href]")?.dataset.href;
    if (!href) return;
    event.preventDefault();
    window.open(href, "_blank", "noopener,noreferrer");
  }
}

const hasFiles = (data: DataTransfer | null) => data?.types.includes("Files") === true;

/** The text offset of the point `x`, `y` in `root`, where a drop landed; none if it isn't in its text. */
function offsetAt(x: number, y: number, root: HTMLElement): number | undefined {
  const position = document.caretPositionFromPoint?.(x, y);
  if (!position || !root.contains(position.offsetNode)) return undefined;
  const range = document.createRange();
  range.selectNodeContents(root);
  range.setEnd(position.offsetNode, position.offset);
  return range.toString().length;
}
