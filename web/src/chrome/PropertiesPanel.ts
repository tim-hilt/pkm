import type { Editor } from "../core/Core";
import { ColorPicker } from "./ColorPicker";
import { backgroundTopPicks, strokeTopPicks } from "./ColorPalette";
import {
  layoutFor,
  propertiesMaxHeight,
  propertiesTop,
  propertiesWidth,
  margin,
} from "./DrawingLayout";
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
 * The desktop Properties panel, on the left beside the Canvas: Excalidraw's
 * 195 px island, as tall as its content and scrolling beyond the height the
 * layout offers. It sets the properties of the Selection, or of the next
 * Element if nothing is selected, and shows what the core says they are; a
 * value the Selection's Elements do not share shows no option chosen. With a
 * Selection it also has its layer and action buttons. The core is the source
 * of truth: `update` re-reads it after every event and patches the DOM. A
 * port of the Swift `PropertiesPanel`.
 */
export class PropertiesPanel {
  readonly element = document.createElement("div");
  private readonly strokeSection: Section;
  private readonly backgroundSection: Section;
  private readonly strokePicker: ColorPicker;
  private readonly backgroundPicker: ColorPicker;
  private readonly groups: {
    group: OptionGroup;
    section: Section;
    buttons: HTMLButtonElement[];
  }[];
  private readonly opacitySection: Section;
  private readonly opacity = document.createElement("input");
  private readonly layers: Section;
  private readonly actions: Section;
  private layout: ReturnType<typeof layoutFor> = "desktop";

  constructor(
    private readonly editor: Editor,
    private readonly onChange: () => void,
    isDark: () => boolean = () => false,
  ) {
    this.element.className = "properties-panel";
    this.element.setAttribute("role", "region");
    this.element.setAttribute("aria-label", "Properties");
    this.element.style.width = `${propertiesWidth}px`;
    this.element.hidden = true;

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
    );
    this.backgroundPicker = new ColorPicker(
      "background",
      "Background",
      backgroundTopPicks,
      (backgroundColor) => apply(() => editor.setProperty({ backgroundColor }))(),
      isDark,
    );
    this.strokeSection = section("Stroke", this.strokePicker.element);
    this.backgroundSection = section("Background", this.backgroundPicker.element);

    // The font size and text align groups share one section of the core.
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

    this.opacity.type = "range";
    this.opacity.min = "0";
    this.opacity.max = "100";
    this.opacity.step = "10";
    this.opacity.setAttribute("aria-label", "Opacity");
    this.opacity.addEventListener(
      "input",
      apply(() => editor.setProperty({ opacity: Number(this.opacity.value) })),
    );
    const ends = document.createElement("div");
    ends.className = "panel-caption panel-ends";
    ends.setAttribute("aria-hidden", "true");
    ends.innerHTML = "<span>0</span><span>100</span>";
    this.opacitySection = section("Opacity", this.opacity, ends);

    this.layers = section(
      "Layers",
      row(layerButtons.map((b) => this.action(b, () => editor.moveInZOrder(b.how)))),
    );
    this.actions = section(
      "Actions",
      row([
        this.action(duplicateButton, () => editor.duplicate()),
        this.action(deleteButton, () => editor.deleteSelection()),
      ]),
    );

    const colorsAndOptions = this.groups.map((g) => g.section.element);
    // Stroke, background, then the options in the panel's order, opacity,
    // layers, actions.
    this.element.append(
      this.strokeSection.element,
      this.backgroundSection.element,
      ...colorsAndOptions,
      this.opacitySection.element,
      this.layers.element,
      this.actions.element,
    );
  }

  /** Shows what the editor says: whether the panel is there, its sections and the chosen options. */
  update(): void {
    const visible = this.layout === "desktop" && this.editor.showsProperties();
    this.element.hidden = !visible;
    if (!visible) return;
    const shown = this.editor.properties();
    const hasSelection = this.editor.selection().length > 0;

    this.strokeSection.element.hidden = !shown.sections.strokeColor;
    this.backgroundSection.element.hidden = !shown.sections.backgroundColor;
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
    this.opacitySection.element.hidden = !shown.sections.opacity;
    this.opacity.value = String(shown.opacity ?? 100);
    this.layers.element.hidden = !hasSelection;
    this.actions.element.hidden = !hasSelection;
  }

  /** Places the panel for a container of this size, below or beside the toolbar of this width: at the layout's top, scrolling beyond its room. The phone layout has no panel. */
  place(containerWidth: number, containerHeight: number, toolbarWidth: number): void {
    this.layout = layoutFor(containerWidth, containerHeight);
    const top = propertiesTop(containerWidth, toolbarWidth);
    this.element.style.left = `${margin}px`;
    this.element.style.top = `${top}px`;
    this.element.style.maxHeight = `${propertiesMaxHeight(containerHeight, top)}px`;
    this.update();
  }

  private action(button: ActionButton, run: () => void): HTMLButtonElement {
    return iconButton(button.name, button.icon, () => {
      run();
      this.onChange();
    });
  }
}

interface Section {
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
