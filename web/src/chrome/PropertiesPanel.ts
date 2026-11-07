import type { Editor } from "../core/Core";
import {
  layoutFor,
  propertiesMaxHeight,
  propertiesTop,
  propertiesWidth,
  margin,
} from "./DrawingLayout";
import { PropertyControls } from "./PropertyControls";

/**
 * The desktop Properties panel, on the left beside the Canvas: Excalidraw's
 * 195 px island, as tall as its content and scrolling beyond the height the
 * layout offers. It stacks the Properties controls, and with a Selection also
 * its layer and action buttons. The core is the source of truth: `update`
 * re-reads it after every event and patches the DOM. A port of the Swift
 * `PropertiesPanel`.
 */
export class PropertiesPanel {
  readonly element = document.createElement("div");
  private readonly controls: PropertyControls;
  private layout: ReturnType<typeof layoutFor> = "desktop";

  constructor(
    private readonly editor: Editor,
    onChange: () => void,
    isDark: () => boolean = () => false,
  ) {
    this.element.className = "properties-panel";
    this.element.setAttribute("role", "region");
    this.element.setAttribute("aria-label", "Properties");
    this.element.style.width = `${propertiesWidth}px`;
    this.element.hidden = true;

    this.controls = new PropertyControls(editor, onChange, isDark);
    const c = this.controls;
    // Stroke, background, then the options in the panel's order, opacity,
    // layers, actions.
    this.element.append(
      c.stroke.element,
      c.background.element,
      ...c.groups.map((g) => g.section.element),
      c.opacity.element,
      c.layers.element,
      c.actions.element,
    );
  }

  /** Shows what the editor says: whether the panel is there, its sections and the chosen options. */
  update(): void {
    const visible = this.layout === "desktop" && this.editor.showsProperties();
    this.element.hidden = !visible;
    if (visible) this.controls.update();
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
}
