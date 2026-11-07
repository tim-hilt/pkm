import type { Editor } from "../core/Core";
import { focusTarget, lockGlyph, toolItems, toolLockLabel } from "./ToolbarModel";

const svg = (glyph: string) =>
  `<svg viewBox="0 0 24 24" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${glyph}</svg>`;

/**
 * The desktop toolbar island, top centre: the Tool lock, then the Tools, as in
 * the native app's `ToolBar`. The core is the source of truth: `update`
 * re-reads the Tool and the lock after every event. Buttons are native, so
 * they are labelled and reachable by keyboard; arrow keys move between them.
 */
export class Toolbar {
  readonly element = document.createElement("div");
  private readonly lock: HTMLButtonElement;
  private readonly tools: HTMLButtonElement[];
  private shownLock: boolean | undefined;
  private shownTool: number | undefined;

  constructor(
    private readonly editor: Editor,
    private readonly onChange: () => void,
  ) {
    this.element.className = "toolbar";
    this.element.setAttribute("role", "toolbar");
    this.element.setAttribute("aria-label", "Tools");

    this.lock = this.button(toolLockLabel, () => editor.setToolLock(!editor.toolLock()));
    this.tools = toolItems.map(({ tool, name, glyph }) => {
      const button = this.button(name, () => editor.setTool(tool));
      button.innerHTML = svg(glyph);
      return button;
    });
    const divider = document.createElement("div");
    divider.className = "toolbar-divider";
    divider.setAttribute("role", "separator");
    this.element.replaceChildren(this.lock, divider, ...this.tools);
    this.element.addEventListener("keydown", (event) => this.moveFocus(event));
    this.update();
  }

  /** Shows the editor's Tool and Tool lock. */
  update(): void {
    const locked = this.editor.toolLock();
    if (locked !== this.shownLock) {
      this.shownLock = locked;
      this.lock.innerHTML = svg(lockGlyph(locked));
      this.lock.setAttribute("aria-pressed", String(locked));
      this.lock.classList.toggle("checked", locked);
    }
    const tool = this.editor.tool();
    if (tool !== this.shownTool) {
      this.shownTool = tool;
      toolItems.forEach((item, index) => {
        const checked = item.tool === tool;
        this.tools[index].setAttribute("aria-pressed", String(checked));
        this.tools[index].classList.toggle("checked", checked);
      });
    }
  }

  private moveFocus(event: KeyboardEvent): void {
    const buttons = [this.lock, ...this.tools];
    const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const target = current < 0 ? undefined : focusTarget(event.key, current, buttons.length);
    if (target === undefined) return;
    event.preventDefault();
    buttons[target].focus();
  }

  private button(label: string, run: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("aria-label", label);
    button.title = label;
    button.addEventListener("click", () => {
      run();
      this.onChange();
    });
    return button;
  }
}
