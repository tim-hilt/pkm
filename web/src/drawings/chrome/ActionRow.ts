import type { Editor } from "../../core/Core";
import { displayColor, isTransparent } from "./ColorPalette";
import { type ActionRowItem, ActionRowModel, opensPopover } from "./ActionRowModel";
import { deleteButton, duplicateButton, type Icon } from "./PropertyGroups";

const svg = (body: string) =>
  `<svg viewBox="0 0 20 20" width="20" height="20" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
const path = (d: string) => svg(`<path d="${d}"/>`);
const glyph = (icon: Icon) => ("svg" in icon ? svg(icon.svg) : icon.text);

const triggers: Record<ActionRowItem, { name: string; icon?: string }> = {
  stroke: { name: "Stroke" },
  background: { name: "Background" },
  properties: { name: "Properties", icon: path("M3 6h8M15 6h2M3 14h2M9 14h8M13 4v4M7 12v4") },
  text: { name: "Text", icon: path("M3 15l4-10 4 10M4.5 11.5h5M13 9h4M15 9v6") },
  actions: {
    name: "Actions",
    icon: svg('<path d="M4 10h.01M10 10h.01M16 10h.01" stroke-width="3"/>'),
  },
  duplicate: { name: duplicateButton.name, icon: glyph(duplicateButton.icon) },
  delete: { name: deleteButton.name, icon: glyph(deleteButton.icon) },
};

/**
 * The phone Action row over the toolbar: one trigger per `ActionRowModel`
 * item. It renders the model and forwards taps; the popovers themselves are
 * placed by `PhoneChrome`. Undo and redo are not here: they sit beside the
 * main menu on the phone.
 */
export class ActionRow {
  readonly element = document.createElement("div");
  private readonly buttons = new Map<ActionRowItem, HTMLButtonElement>();
  private shownItems = "";

  constructor(
    private readonly editor: Editor,
    private readonly model: ActionRowModel,
    private readonly handlers: { tap(item: ActionRowItem): void; changed(): void },
    private readonly isDark: () => boolean = () => false,
  ) {
    this.element.className = "action-row";
    this.element.setAttribute("role", "toolbar");
    this.element.setAttribute("aria-label", "Actions");
  }

  /** The trigger of an item that is on the row, for placing its popover. */
  trigger(item: ActionRowItem): HTMLElement | undefined {
    return this.buttons.get(item);
  }

  /** Shows the model: its items and the open one's checked state. */
  update(): void {
    const items = this.model.items;
    const key = items.join();
    if (key !== this.shownItems) {
      this.shownItems = key;
      this.element.replaceChildren(...items.map((item) => this.button(item)));
    }
    const shown =
      items.includes("stroke") || items.includes("background")
        ? this.editor.properties()
        : undefined;
    for (const item of items) {
      const button = this.buttons.get(item)!;
      const open = this.model.open === item;
      button.classList.toggle("checked", open);
      if (opensPopover(item)) button.setAttribute("aria-expanded", String(open));
      if (item === "stroke" || item === "background") {
        const color = shown?.[item === "stroke" ? "strokeColor" : "backgroundColor"];
        const current = color ?? "transparent";
        button.style.setProperty("--swatch", displayColor(current, this.isDark()));
        button.classList.toggle("transparent", isTransparent(current));
      }
    }
  }

  private button(item: ActionRowItem): HTMLButtonElement {
    let button = this.buttons.get(item);
    if (button) return button;
    const { name, icon } = triggers[item];
    button = document.createElement("button");
    button.type = "button";
    button.className = icon ? "row-button" : "row-button row-swatch";
    button.title = name;
    button.setAttribute("aria-label", name);
    if (icon) button.innerHTML = icon;
    else if (item === "stroke") button.innerHTML = path("M3 15L15 3M3 10l7-7M8 17L17 8");
    button.addEventListener("click", () => {
      if (item === "duplicate") this.editor.duplicate();
      else if (item === "delete") this.editor.deleteSelection();
      else this.handlers.tap(item);
      this.handlers.changed();
    });
    this.buttons.set(item, button);
    return button;
  }
}
