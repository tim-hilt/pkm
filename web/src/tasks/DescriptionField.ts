import { rawRange } from "../core/Core";
import { make } from "../Dom";
import { type Raw, descriptionNodes } from "./DescriptionText";
import { caretOffset, placeCaret, selectionOffsets } from "./TextCaret";

/**
 * The Description of a Task, edited in Live Preview: its model is the Markdown
 * string, which is all the element holds. The browser edits the text; after each
 * edit, and when the caret moves to another line, the field is drawn again from
 * the core's parse, with the caret kept. Every line is rendered but the one
 * with the caret (the whole fence, in a code block), which shows its markers.
 * Rendered links open on a click; pasting inserts plain text.
 */
export class DescriptionField {
  /** The `contenteditable` element, which sends `input` events like a text field. */
  readonly element = make("div", "tasks-input tasks-description");
  /** Text and raw range as last drawn, to draw only what changed. */
  private drawn: string | undefined;
  private composing = false;

  constructor(text = "") {
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
      if (!(event as InputEvent).isComposing) this.draw();
    });
    editor.addEventListener("compositionstart", () => (this.composing = true));
    editor.addEventListener("compositionend", () => {
      this.composing = false;
      this.draw();
    });
    editor.addEventListener("beforeinput", (event) => this.lineBreak(event));
    editor.addEventListener("mousedown", (event) => this.open(event));
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
  }

  /** Takes the keyboard focus. */
  focus(): void {
    this.element.focus();
  }

  /** Return inserts a line break itself: the browser's leaves a second one after the last line. */
  private lineBreak(event: InputEvent): void {
    if (event.inputType !== "insertParagraph" && event.inputType !== "insertLineBreak") return;
    event.preventDefault();
    const selected = selectionOffsets(this.element);
    if (!selected) return;
    const text = this.value;
    this.draw(text.slice(0, selected.start) + "\n" + text.slice(selected.end), selected.start + 1);
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
    const nodes = descriptionNodes(text, raw);
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
