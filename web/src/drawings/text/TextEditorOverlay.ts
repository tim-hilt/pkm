import type { TextEditor } from "../../core/Core";
import type { TextMeasurer } from "./Excalifont";
import { caretLine, type Rect } from "./KeyboardAvoidance";
import { caretRoom, textViewStyle } from "./TextViewStyle";

/**
 * The Text view shown over the Canvas while a text is edited (ADR-0006), a
 * `<textarea>` like Excalidraw's, a port of the Swift `TextEditorOverlay`. The
 * core says where it is and what it looks like (`TextEditor`, in the Canvas's
 * coordinates); this keeps a textarea there, so that the system keyboard,
 * autocorrect and dictation work, and reports what is typed and when the
 * editing ends.
 */
export class TextEditorOverlay {
  /** What the view says, after every change. */
  onChange?: (text: string) => void;
  /** The editing ended from the view: it lost the focus (the keyboard was dismissed, another control was used), or Escape or Ctrl/⌘ Enter was pressed. */
  onEnd?: () => void;

  private view?: HTMLTextAreaElement;
  private shown?: string;

  /** Whether a text is edited. */
  get isOpen(): boolean {
    return this.view !== undefined;
  }

  /** What the view says. */
  get text(): string {
    return this.view?.value ?? "";
  }

  /**
   * Shows the view as `editor` says, in `host` (whose top left corner is the
   * Canvas's): opens it, with the focus, if it is not open yet. What is typed
   * in it stays; only where it is and how it looks follows the editor.
   */
  show(editor: TextEditor, host: HTMLElement): void {
    const key = JSON.stringify(editor);
    if (key === this.shown) return;
    this.shown = key;
    const view = this.view ?? this.open(host);
    // The core knows what was typed; it differs only when another text opens.
    if (view.value !== editor.text) view.value = editor.text;
    view.wrap = editor.fixedWidth ? "soft" : "off";
    Object.assign(view.style, textViewStyle(editor));
    if (!this.view) {
      this.view = view;
      host.append(view);
      view.focus({ preventScroll: true });
    }
  }

  /** Takes the view away. */
  hide(): void {
    const view = this.view;
    if (!view) return;
    // Taking the focus away is not the editing ending from the view.
    this.view = undefined;
    this.shown = undefined;
    view.remove();
  }

  /**
   * The caret in the Canvas's coordinates, measured by `font`, for keeping it
   * in view and above the keyboard.
   */
  caret(editor: TextEditor, font: TextMeasurer): Rect | undefined {
    const view = this.view;
    if (!view) return undefined;
    const box = { ...editor, lineHeight: editor.fontSize * editor.lineHeight };
    const lineWidth = (text: string) => font.lineWidth(text, editor.fontSize);
    return caretLine(box, view.value, view.selectionEnd, lineWidth);
  }

  private open(host: HTMLElement): HTMLTextAreaElement {
    const view = host.ownerDocument.createElement("textarea");
    view.className = "text-view";
    view.setAttribute("aria-label", "Text");
    view.rows = 1;
    view.style.padding = `0 ${caretRoom}px`;
    view.addEventListener("input", () => {
      if (view === this.view) this.onChange?.(view.value);
    });
    view.addEventListener("blur", () => {
      if (view === this.view) this.onEnd?.();
    });
    // Escape and Ctrl/⌘ Enter end the editing, as on excalidraw.com.
    view.addEventListener("keydown", (e) => {
      if (e.key === "Escape" || (e.key === "Enter" && (e.metaKey || e.ctrlKey))) {
        e.preventDefault();
        if (view === this.view) this.onEnd?.();
      }
    });
    return view;
  }
}
