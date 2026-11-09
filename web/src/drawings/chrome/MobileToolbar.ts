import type { Editor, Tool } from "../../core/Core";
import {
  type Glyph,
  glyphSvg,
  insertImageItem,
  lockGlyph,
  mobileToolSlots,
  shownTool,
  toolItems,
  toolLockLabel,
} from "./ToolbarModel";

const svg = (glyph: Glyph) => glyphSvg(glyph, 20);
const item = (tool: Tool) => toolItems.find((i) => i.tool === tool)!;

/**
 * The phone toolbar island (Excalidraw's bottom bar): one slot per Tool
 * family, spread over the bar's width; tapping a family's armed Tool again
 * asks to open the family's picker. Tool lock, which has no slot on
 * excalidraw.com's phone bar, is an icon button at the end. A port of the
 * Swift `MobileToolBar`.
 */
export class MobileToolbar {
  readonly element = document.createElement("div");
  private readonly slots: HTMLButtonElement[];
  private readonly lock: HTMLButtonElement;
  /** Per slot, the Tool of its family last chosen, so the slot keeps showing it. */
  private readonly lastChosen = new Map<number, Tool>();
  private readonly shown: (Tool | undefined)[] = [];

  constructor(
    private readonly editor: Editor,
    private readonly handlers: {
      /** The slot's armed Tool was tapped again: its family's picker should open. */
      openFamily(slot: number): void;
      /** Any other tap, which leaves open pickers behind. */
      changed(): void;
      /** The Insert image button was tapped. */
      insertImages(): void;
    },
  ) {
    this.element.className = "mobile-toolbar";
    this.element.setAttribute("role", "toolbar");
    this.element.setAttribute("aria-label", "Tools");
    this.slots = mobileToolSlots.map((slot, index) =>
      this.button(item(slot.tools[0]).name, () => {
        const tool = this.shownTool(index);
        if (editor.tool() === tool && slot.tools.length > 1) this.handlers.openFamily(index);
        else {
          editor.setTool(tool);
          this.handlers.changed();
        }
      }),
    );
    this.lock = this.button(toolLockLabel, () => {
      editor.setToolLock(!editor.toolLock());
      this.handlers.changed();
    });
    const insertImages = this.button(insertImageItem.name, () => {
      this.handlers.insertImages();
      this.handlers.changed();
    });
    insertImages.innerHTML = svg(insertImageItem.glyph);
    this.element.replaceChildren(...this.slots, this.lock, insertImages);
    this.update();
  }

  /** The slot's button, which a family picker is anchored to. */
  slotButton(slot: number): HTMLElement {
    return this.slots[slot];
  }

  /** Arms `tool` as the choice of the picker of `slot`, which the slot keeps showing. */
  choose(slot: number, tool: Tool): void {
    this.lastChosen.set(slot, tool);
    this.editor.setTool(tool);
  }

  /** Shows the editor's Tool and Tool lock. */
  update(): void {
    const current = this.editor.tool();
    mobileToolSlots.forEach((_, index) => {
      const tool = this.shownTool(index);
      const button = this.slots[index];
      if (tool !== this.shown[index]) {
        this.shown[index] = tool;
        const { name, glyph } = item(tool);
        button.innerHTML = svg(glyph);
        button.title = name;
        button.setAttribute("aria-label", name);
      }
      const armed = tool === current;
      button.setAttribute("aria-pressed", String(armed));
      button.classList.toggle("checked", armed);
    });
    const locked = this.editor.toolLock();
    this.lock.innerHTML = svg(lockGlyph(locked));
    this.lock.setAttribute("aria-pressed", String(locked));
    this.lock.classList.toggle("checked", locked);
  }

  /** The Tools of a slot's family with their buttons' contents, for its picker. */
  family(slot: number): { tool: Tool; name: string; glyph: string; checked: boolean }[] {
    const current = this.editor.tool();
    return mobileToolSlots[slot].tools.map((tool) => {
      const { name, glyph } = item(tool);
      return { tool, name, glyph: svg(glyph), checked: tool === current };
    });
  }

  private shownTool(slot: number): Tool {
    return shownTool(mobileToolSlots[slot], this.editor.tool(), this.lastChosen.get(slot));
  }

  private button(label: string, click: () => void): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.title = label;
    button.setAttribute("aria-label", label);
    button.addEventListener("click", click);
    return button;
  }
}
