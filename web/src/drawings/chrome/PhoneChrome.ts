import type { Editor } from "../../core/Core";
import { ActionRow } from "./ActionRow";
import { type ActionRowItem, ActionRowModel } from "./ActionRowModel";
import { bottomBarWidth } from "./DrawingLayout";
import { MobileToolbar } from "./MobileToolbar";
import { PhonePopover } from "./PhonePopover";
import { PropertyControls } from "./PropertyControls";

/**
 * excalidraw.com's phone layout: everything in the bottom bar within thumb
 * reach, properties in popovers. The Action row sits over the toolbar; each
 * of its triggers opens a popover with the part of the Properties panel of
 * its concern, and the toolbar's Tool families open pickers. At most one
 * popover is open. The chrome is always in the DOM and shown by the phone
 * media query (`phoneQuery`), so it follows resizing and rotation without
 * script; `place` only gives it the bar's width, for the Action row. A port of
 * the Swift `mobileChrome`.
 */
export class PhoneChrome {
  readonly element = document.createElement("div");
  private readonly bar = document.createElement("div");
  private readonly model = new ActionRowModel();
  private readonly row: ActionRow;
  private readonly toolbar: MobileToolbar;
  private readonly controls: PropertyControls;
  private readonly popover: PhonePopover;
  private openFamily: number | undefined;
  private shownPopover: string | undefined;
  private width = 0;

  constructor(
    private readonly editor: Editor,
    private readonly onChange: () => void,
    isDark: () => boolean = () => false,
    /** Asks the user for images to insert. */
    insertImages: () => void = () => {},
  ) {
    this.element.className = "phone-chrome";
    this.bar.className = "bottom-bar";
    this.controls = new PropertyControls(editor, onChange, isDark, true);
    this.row = new ActionRow(
      editor,
      this.model,
      {
        tap: (item) => {
          this.openFamily = undefined;
          this.model.tap(item);
        },
        changed: onChange,
      },
      isDark,
    );
    this.toolbar = new MobileToolbar(editor, {
      openFamily: (slot) => {
        this.model.close();
        this.openFamily = this.openFamily === slot ? undefined : slot;
        this.onChange();
      },
      changed: () => {
        this.close();
        this.onChange();
      },
      insertImages,
    });
    this.popover = new PhonePopover(this.element, () => {
      this.close();
      this.onChange();
    });
    this.bar.append(this.row.element, this.toolbar.element);
    this.element.append(this.popover.scrim, this.bar, this.popover.element);
  }

  /** Sizes the bar for a container of this width. */
  place(containerWidth: number): void {
    this.width = bottomBarWidth(containerWidth);
    this.bar.style.width = `${this.width}px`;
    this.update();
  }

  /** Shows what the editor says: the Action row's items, the toolbar and the open popover. */
  update(): void {
    this.model.update({
      sections: this.editor.showsProperties() ? this.editor.properties().sections : undefined,
      hasSelection: this.editor.selection().length > 0,
      width: this.width,
    });
    this.row.update();
    this.toolbar.update();
    this.showPopover();
  }

  private close(): void {
    this.openFamily = undefined;
    this.model.close();
  }

  private showPopover(): void {
    const family = this.openFamily;
    const item = this.model.open;
    const key = family !== undefined ? `family ${family}` : item;
    if (key !== this.shownPopover) {
      this.shownPopover = key;
      if (family !== undefined) {
        const picker = this.familyPicker(family);
        this.popover.show("Tools", [picker], this.toolbar.slotButton(family), this.toolbar.element);
      } else if (item) {
        const content = this.content(item);
        this.popover.show(item, [content], this.row.trigger(item)!, this.row.element);
      } else {
        this.popover.hide();
      }
    } else if (item) {
      // The same popover: its controls follow the editor in place (a slider being dragged stays), and its size may change.
      this.controls.update();
      if (item === "actions") this.offerActions();
      this.popover.place();
    }
  }

  private offerActions(): void {
    this.controls.offerActions(this.model.popoverHasDuplicate, this.model.popoverHasDelete);
  }

  private familyPicker(slot: number): HTMLElement {
    const picker = document.createElement("div");
    picker.className = "family-picker";
    for (const { tool, name, glyph, checked } of this.toolbar.family(slot)) {
      const button = document.createElement("button");
      button.type = "button";
      button.title = name;
      button.setAttribute("aria-label", name);
      button.setAttribute("aria-pressed", String(checked));
      button.classList.toggle("checked", checked);
      button.innerHTML = glyph;
      button.addEventListener("click", () => {
        this.toolbar.choose(slot, tool);
        this.close();
        this.onChange();
      });
      picker.append(button);
    }
    return picker;
  }

  /** The popover of a trigger: the controls of its concern, from the same pieces as the desktop panel. */
  private content(item: ActionRowItem): HTMLElement {
    const c = this.controls;
    c.update();
    const content = document.createElement("div");
    content.className = "popover-sections";
    switch (item) {
      case "stroke":
        content.style.width = "196px";
        content.append(c.stroke.element);
        break;
      case "background":
        content.style.width = "196px";
        content.append(c.background.element);
        break;
      case "properties":
        content.style.width = "176px";
        content.append(
          ...c.groupSections("Fill", "Stroke width", "Stroke style", "Sloppiness", "Edges"),
          c.opacity.element,
        );
        break;
      case "text":
        content.append(...c.groupSections("Font size", "Text align"));
        break;
      case "actions":
        this.offerActions();
        content.append(c.layers.element, c.actions.element);
        break;
    }
    return content;
  }
}
