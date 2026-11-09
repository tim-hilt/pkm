import { beforeAll, describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { loadCore } from "../../core/Core";
import { Excalifont } from "../text/Excalifont";
import {
  backgroundTopPicks,
  colorFromHex,
  colorName,
  displayColor,
  grid,
  shadesOf,
  strokeTopPicks,
} from "./ColorPalette";

// `displayColor` asks the core for its dark colour, so the wasm is loaded.
beforeAll(async () => {
  const generated = new URL("../../../generated/", import.meta.url);
  const font = new Excalifont(
    new Uint8Array(readFileSync(new URL("Excalifont-Regular.ttf", generated))),
  );
  await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)), font);
});

describe("the colour palette", () => {
  it("has Excalidraw's fifteen colours in the dark shade for strokes", () => {
    const colors = grid("stroke");
    expect(colors).toHaveLength(15);
    expect(colors.slice(0, 5)).toEqual(["transparent", "#ffffff", "#343a40", "#1e1e1e", "#846358"]);
    expect(colors[5]).toBe("#0c8599"); // cyan, shade 4
    expect(colors[14]).toBe("#e03131"); // red, shade 4
  });

  it("uses the light shade for backgrounds", () => {
    const colors = grid("background");
    expect(colors[2]).toBe("#e9ecef"); // gray, shade 1
    expect(colors[14]).toBe("#ffc9c9"); // red, shade 1
    expect(colors[0]).toBe("transparent");
  });

  it("has every top pick in the palette", () => {
    for (const color of strokeTopPicks.filter((c) => c !== "#1e1e1e")) {
      expect(shadesOf(color), color).toBeDefined();
    }
    for (const color of backgroundTopPicks.filter((c) => c !== "transparent")) {
      expect(shadesOf(color), color).toBeDefined();
    }
  });

  it("gives a palette colour its five shades", () => {
    expect(shadesOf("#e03131")).toEqual(["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"]);
    expect(shadesOf("#FF8787")).toHaveLength(5);
  });

  it("gives other colours no shades", () => {
    expect(shadesOf("#123456")).toBeUndefined();
    expect(shadesOf("transparent")).toBeUndefined();
    expect(shadesOf("#ffffff")).toBeUndefined();
    expect(shadesOf(undefined)).toBeUndefined();
  });

  it("normalises hex input like Excalidraw", () => {
    expect(colorFromHex("E03131")).toBe("#e03131");
    expect(colorFromHex(" #E03131 ")).toBe("#e03131");
    expect(colorFromHex("f00")).toBe("#f00");
    expect(colorFromHex("f008")).toBe("#f008");
    expect(colorFromHex("ff000080")).toBe("#ff000080");
  });

  it("rejects invalid hex input", () => {
    for (const input of ["", "ff", "fffff", "gggggg", "#1234567"]) {
      expect(colorFromHex(input), input).toBeUndefined();
    }
  });

  it("has a spoken name for every palette colour", () => {
    for (const kind of ["stroke", "background"] as const) {
      for (const color of grid(kind).flatMap((c) => shadesOf(c) ?? [c])) {
        expect(colorName(color).startsWith("Colour"), color).toBe(false);
      }
    }
    expect(colorName("#1e1e1e")).toBe("Black");
    expect(colorName("#ff8787")).toBe("Red, shade 3");
    expect(colorName("#123456")).toBe("Colour #123456");
  });

  it("shows the plain colour in light mode and the core's dark colour in dark mode", () => {
    expect(displayColor("#1e1e1e", false)).toBe("#1e1e1e");
    expect(displayColor("transparent", false)).toBe("transparent");
    expect(displayColor("#1e1e1e", true)).not.toBe("#1e1e1e");
  });
});
