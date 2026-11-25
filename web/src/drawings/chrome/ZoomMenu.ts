import { MenuButton } from "./MenuButton";
import { type Zoom, zoomPercent } from "./ZoomControls";

/**
 * The phone's zoom controls, beside the main menu: the zoom in percent,
 * opening a menu of the zoom group's commands.
 */
export class ZoomMenu {
  private readonly menu: MenuButton;
  readonly element: HTMLElement;

  constructor(
    private readonly zoom: Zoom,
    onChange: () => void,
  ) {
    this.menu = new MenuButton("Zoom", () => {
      this.update();
      onChange();
    });
    this.element = this.menu.element;
    this.element.classList.add("zoom-menu");
    this.menu.set(
      this.menu.item("Reset", "menuitem", () => zoom.resetZoom()),
      this.menu.item("Zoom to Content", "menuitem", () => zoom.zoomToFit()),
    );
    this.update();
  }

  /** Shows the zoom. */
  update(): void {
    const text = `${zoomPercent(this.zoom.viewport().zoom)}%`;
    if (this.menu.toggle.textContent !== text) {
      this.menu.toggle.textContent = text;
      this.menu.toggle.setAttribute("aria-valuetext", text);
    }
  }

  dispose(): void {
    this.menu.hide();
  }
}
