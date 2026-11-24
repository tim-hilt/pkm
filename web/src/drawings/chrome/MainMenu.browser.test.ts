import { describe, expect, it } from "vitest";
import { MainMenu, type MainMenuActions } from "./MainMenu";

describe("MainMenu", () => {
  const state = { dimensions: false, canSetUnit: false, canResetUnit: false, log: [] as string[] };
  const actions: MainMenuActions = {
    saveAs() {},
    exportImage() {},
    canExport: () => true,
    pencilOnlyDrawing: () => false,
    setPencilOnlyDrawing() {},
    showDimensions: () => state.dimensions,
    setShowDimensions: (on) => (state.dimensions = on),
    canSetUnit: () => state.canSetUnit,
    canResetUnit: () => state.canResetUnit,
    askUnit: () => state.log.push("ask"),
    resetUnit: () => state.log.push("reset"),
  };
  const menu = new MainMenu(actions);
  document.body.append(menu.element);
  const item = (label: string) =>
    [...menu.element.querySelectorAll<HTMLButtonElement>("[role^=menuitem]")].find(
      (i) => i.textContent === label,
    )!;

  it("has a Show dimensions checkbox, off at first, and no Unit items", () => {
    expect(item("Show dimensions").getAttribute("role")).toBe("menuitemcheckbox");
    expect(item("Show dimensions").getAttribute("aria-checked")).toBe("false");
    expect(item("Set unit…").hidden).toBe(true);
    expect(item("Reset unit").hidden).toBe(true);
  });

  it("shows the Unit items while Dimensions are on, enabled by the Selection and the Unit", () => {
    item("Show dimensions").click();
    expect(item("Show dimensions").getAttribute("aria-checked")).toBe("true");
    expect(item("Set unit…").hidden).toBe(false);
    expect(item("Set unit…").disabled).toBe(true);
    expect(item("Reset unit").disabled).toBe(true);

    state.canSetUnit = state.canResetUnit = true;
    menu.update();
    expect(item("Set unit…").disabled).toBe(false);
    expect(item("Reset unit").disabled).toBe(false);
    item("Set unit…").click();
    item("Reset unit").click();
    expect(state.log).toEqual(["ask", "reset"]);
  });

  it("takes the Unit items away again when Dimensions go off", () => {
    item("Show dimensions").click();
    expect(item("Set unit…").hidden).toBe(true);
  });
});
