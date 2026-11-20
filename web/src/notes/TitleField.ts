import { type Inline, toggleInline } from "../core/Core";
import { make } from "../Dom";
import { caretOffset, placeCaret, selectRange, selectionOffsets } from "../tasks/TextCaret";
import { formatKey, titleNodes } from "../tasks/TitleText";

/**
 * The Title of a Note: one line of text with Inline formatting, raw while the
 * field is focused (the CSS shows its markers then) and formatted otherwise.
 * ⌘B, ⌘I and ⌘E toggle formatting; Return calls `submit` instead of breaking
 * the line. There are no Quick Add Shortcuts: `@work` and `p1` stay text.
 */
export class TitleField {
  readonly element = make("div", "tasks-input notes-title");
  private drawn: string | undefined;
  private composing = false;

  constructor(
    text: string,
    private readonly options: { change: () => void; submit: () => void },
  ) {
    const editor = this.element;
    editor.setAttribute("contenteditable", "plaintext-only");
    editor.setAttribute("role", "textbox");
    editor.setAttribute("aria-label", "Title");
    editor.dataset.placeholder = "Title";
    editor.autocapitalize = "sentences";
    editor.spellcheck = true;
    this.draw(text);
    editor.addEventListener("input", (event) => {
      if ((event as InputEvent).isComposing) return;
      this.draw();
      options.change();
    });
    editor.addEventListener("compositionstart", () => (this.composing = true));
    editor.addEventListener("compositionend", () => {
      this.composing = false;
      this.draw();
      options.change();
    });
    editor.addEventListener("beforeinput", (event) => {
      if (event.inputType === "insertParagraph" || event.inputType === "insertLineBreak") {
        event.preventDefault();
        options.submit();
      }
    });
    editor.addEventListener("keydown", (event) => {
      const kind = event.isComposing ? undefined : formatKey(event);
      if (kind === undefined) return;
      event.preventDefault();
      this.format(kind);
    });
  }

  /** The Title as typed, on one line. */
  get value(): string {
    return (this.element.textContent ?? "").replace(/[\r\n]+/g, " ");
  }

  set value(text: string) {
    this.draw(text);
  }

  focus(): void {
    this.element.focus();
  }

  /** Toggles Inline formatting on the selection, which stays selected. */
  private format(kind: Inline): void {
    const range = selectionOffsets(this.element);
    if (!range) return;
    const edit = toggleInline(this.value, range.start, range.end, kind);
    const text = this.value;
    this.draw(text.slice(0, edit.start) + edit.replacement + text.slice(edit.end));
    selectRange(this.element, edit.selectionStart, edit.selectionEnd);
    this.options.change();
  }

  /** Draws `text` formatted, the caret kept; only if it changed. */
  private draw(text = this.value): void {
    if (this.composing || text === this.drawn) return;
    this.drawn = text;
    const caret = document.activeElement === this.element ? caretOffset(this.element) : undefined;
    this.element.replaceChildren(...titleNodes(text));
    if (caret !== undefined) placeCaret(this.element, caret);
  }
}
