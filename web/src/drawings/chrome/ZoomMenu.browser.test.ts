import { afterEach, describe, expect, it } from "vitest";
import { ZoomMenu } from "./ZoomMenu";

describe("ZoomMenu", () => {
  const zoom = {
    zoom: 1,
    resetZoom() {
      this.zoom = 1;
    },
    zoomToFit() {
      this.zoom = 0.25;
    },
    viewport() {
      return { zoom: this.zoom };
    },
  };
  let changes = 0;
  const menu = new ZoomMenu(zoom, () => changes++);
  document.body.append(menu.element);
  const toggle = () => menu.element.querySelector<HTMLButtonElement>("[aria-haspopup=menu]")!;
  const items = () => [...menu.element.querySelectorAll<HTMLButtonElement>("[role=menuitem]")];
  const pick = (label: string) => {
    toggle().click();
    items()
      .find((item) => item.textContent === label)!
      .click();
  };

  afterEach(() => {
    zoom.zoom = 1;
    changes = 0;
    menu.update();
  });

  it("shows the zoom in percent on its button", () => {
    expect(toggle().textContent).toBe("100%");
    expect(toggle().getAttribute("aria-label")).toBe("Zoom");
    expect(toggle().getAttribute("aria-valuetext")).toBe("100%");
  });

  it("offers the zoom group's commands", () => {
    toggle().click();
    expect(items().map((item) => item.textContent)).toEqual([
      "Reset",
      "Zoom to Content",
    ]);
  });

  it("runs a command, shows the new zoom and closes", () => {
    pick("Zoom to Content");
    expect(toggle().textContent).toBe("25%");
    expect(changes).toBe(1);
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    pick("Reset");
    expect(toggle().textContent).toBe("100%");
  });
});
