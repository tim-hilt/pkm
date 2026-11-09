import type { Editor } from "../../core/Core";

const arrow = (flip: boolean) =>
  `<svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"${flip ? ' style="transform:scaleX(-1)"' : ""}><path d="M9 14L4 9l5-5M4 9h10a6 6 0 0 1 0 12h-3"/></svg>`;

/**
 * Excalidraw's undo and redo buttons, next to the zoom controls. They are
 * enabled exactly when the core can undo or redo, which `update` re-reads
 * after every event.
 */
export class UndoRedoControls {
  readonly element = document.createElement("div");
  private readonly undo: HTMLButtonElement;
  private readonly redo: HTMLButtonElement;

  constructor(
    private readonly editor: Editor,
    private readonly onChange: () => void,
  ) {
    this.element.className = "undo-redo-controls";
    this.element.setAttribute("role", "group");
    this.element.setAttribute("aria-label", "History");
    this.undo = this.button("Undo", false, () => editor.undo());
    this.redo = this.button("Redo", true, () => editor.redo());
    this.element.replaceChildren(this.undo, this.redo);
    this.update();
  }

  update(): void {
    this.undo.disabled = !this.editor.canUndo();
    this.redo.disabled = !this.editor.canRedo();
  }

  private button(label: string, flip: boolean, run: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.innerHTML = arrow(flip);
    button.setAttribute("aria-label", label);
    button.title = label;
    button.addEventListener("click", () => {
      run();
      this.onChange();
    });
    return button;
  }
}
