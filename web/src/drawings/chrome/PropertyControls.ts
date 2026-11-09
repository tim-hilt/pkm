import type { Editor } from "../../core/Core";
import { ColorPicker } from "./ColorPicker";
import { backgroundTopPicks, strokeTopPicks } from "./ColorPalette";
import {
  type ActionButton,
  type Icon,
  type OptionGroup,
  deleteButton,
  duplicateButton,
  layerButtons,
  optionGroups,
} from "./PropertyGroups";

const iconHtml = (icon: Icon) =>
  "svg" in icon
    ? `<svg viewBox="0 0 20 20" width="20" height="20" aria-hidden="true">${icon.svg}</svg>`
    : `<span aria-hidden="true">${icon.text}</span>`;

/**
 * The controls of the Properties panel, one titled section per concern, shared
 * by the desktop panel (which stacks them all) and the phone's popovers (which
 * pick the ones they need). They set the properties of the Selection, or of
 * the next Element if nothing is selected, and show what the core says they
 * are; a value the Selection's Elements do not share shows no option chosen.
 * `update` re-reads the editor and patches the DOM.
 */
export class PropertyControls {
  readonly stroke: Section;
  readonly background: Section;
  /** The option groups by title; the font size and text align groups are the text ones. */
  readonly groups: { group: OptionGroup; section: Section; buttons: HTMLButtonElement[] }[];
  readonly opacity: Section;
  readonly layers: Section;
  readonly actions: Section;
  private readonly strokePicker: ColorPicker;
  private readonly backgroundPicker: ColorPicker;
  private readonly opacityInput = document.createElement("input");
  private readonly duplicate: HTMLButtonElement;
  private readonly delete: HTMLButtonElement;

  /** `colorsInFull`: the colour pickers show their full palette at once, without top picks. */
  constructor(
    private readonly editor: Editor,
    private readonly onChange: () => void,
    isDark: () => boolean = () => false,
    colorsInFull = false,
  ) {
    const apply = (run: () => void) => () => {
      run();
      this.onChange();
    };
    this.strokePicker = new ColorPicker(
      "stroke",
      "Stroke",
      strokeTopPicks,
      (strokeColor) => apply(() => editor.setProperty({ strokeColor }))(),
      isDark,
      colorsInFull,
    );
    this.backgroundPicker = new ColorPicker(
      "background",
      "Background",
      backgroundTopPicks,
      (backgroundColor) => apply(() => editor.setProperty({ backgroundColor }))(),
      isDark,
      colorsInFull,
    );
    this.stroke = section("Stroke", this.strokePicker.element);
    this.background = section("Background", this.backgroundPicker.element);

    this.groups = optionGroups.map((group) => {
      const buttons = group.options.map((option) =>
        iconButton(
          option.name,
          option.icon,
          apply(() => editor.setProperty(group.property(option.value))),
        ),
      );
      return { group, section: section(group.title, row(buttons)), buttons };
    });

    const input = this.opacityInput;
    input.type = "range";
    input.min = "0";
    input.max = "100";
    input.step = "10";
    input.setAttribute("aria-label", "Opacity");
    input.addEventListener(
      "input",
      apply(() => editor.setProperty({ opacity: Number(input.value) })),
    );
    const ends = document.createElement("div");
    ends.className = "panel-caption panel-ends";
    ends.setAttribute("aria-hidden", "true");
    ends.innerHTML = "<span>0</span><span>100</span>";
    this.opacity = section("Opacity", input, ends);

    const action = (button: ActionButton, run: () => void) =>
      iconButton(button.name, button.icon, apply(run));
    this.layers = section(
      "Layers",
      row(layerButtons.map((b) => action(b, () => editor.moveInZOrder(b.how)))),
    );
    this.duplicate = action(duplicateButton, () => editor.duplicate());
    this.delete = action(deleteButton, () => editor.deleteSelection());
    this.actions = section("Actions", row([this.duplicate, this.delete]));
  }

  /** The sections of the option groups with these titles. */
  groupSections(...titles: string[]): HTMLElement[] {
    return this.groups.filter((g) => titles.includes(g.group.title)).map((g) => g.section.element);
  }

  /** Whether the Actions section offers Duplicate and Delete (they may be on the phone's Action row instead). */
  offerActions(duplicate: boolean, del: boolean): void {
    this.duplicate.hidden = !duplicate;
    this.delete.hidden = !del;
  }

  /** Shows what the editor says: which sections apply and the chosen options. */
  update(): void {
    const shown = this.editor.properties();
    const hasSelection = this.editor.selection().length > 0;

    this.stroke.element.hidden = !shown.sections.strokeColor;
    this.background.element.hidden = !shown.sections.backgroundColor;
    this.strokePicker.update(shown.strokeColor);
    this.backgroundPicker.update(shown.backgroundColor);
    for (const { group, section, buttons } of this.groups) {
      section.element.hidden = !group.applies(shown);
      const chosen = group.chosen(shown);
      group.options.forEach((option, index) => {
        const on = option.value === chosen;
        buttons[index].setAttribute("aria-pressed", String(on));
        buttons[index].classList.toggle("selected", on);
      });
    }
    this.opacity.element.hidden = !shown.sections.opacity;
    this.opacityInput.value = String(shown.opacity ?? 100);
    this.layers.element.hidden = !hasSelection;
    this.actions.element.hidden = !hasSelection;
  }
}

export interface Section {
  element: HTMLElement;
}

/** A titled group: a caption above its content. */
function section(title: string, ...content: HTMLElement[]): Section {
  const element = document.createElement("div");
  element.className = "panel-section";
  const caption = document.createElement("div");
  caption.className = "panel-caption";
  caption.textContent = title;
  element.append(caption, ...content);
  return { element };
}

function row(children: HTMLElement[]): HTMLElement {
  const element = document.createElement("div");
  element.className = "icon-row";
  element.append(...children);
  return element;
}

/** One 32 px icon button of the panel (hit area grown to 44 px). */
function iconButton(name: string, icon: Icon, click: () => void): HTMLButtonElement {
  const button = document.createElement("button");
  button.type = "button";
  button.className = "icon-button";
  button.title = name;
  button.setAttribute("aria-label", name);
  button.innerHTML = iconHtml(icon);
  button.addEventListener("click", click);
  return button;
}
