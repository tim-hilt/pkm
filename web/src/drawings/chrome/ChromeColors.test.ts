import { describe, expect, it } from "vitest";
import { applyChromeColors, chromeColors } from "./ChromeColors";

/** The chrome's colours in the two appearances: Excalidraw's light values, and its dark theme's in Dark mode. A port of the Swift `ChromeColorsTests`. */
describe("the chrome's colours", () => {
  it("are white islands in Light and Excalidraw's dark grey in Dark", () => {
    expect(chromeColors(false)["--island-fill"]).toBe("#ffffff");
    expect(chromeColors(true)["--island-fill"]).toBe("#232329");
  });

  it("keep their light checked colours and use the dark accent's in Dark", () => {
    const light = chromeColors(false);
    const dark = chromeColors(true);
    expect([light["--checked-fill"], light["--checked-ink"]]).toEqual(["#e0dfff", "#030064"]);
    expect([dark["--checked-fill"], dark["--checked-ink"]]).toEqual(["#403e6a", "#e0dfff"]);
  });

  it("have Excalidraw's purple as the accent in both", () => {
    expect(chromeColors(false)["--accent"]).toBe("#6965db");
    expect(chromeColors(true)["--accent"]).toBe("#a8a5ff");
  });

  it("are the same set of colours in both appearances", () => {
    expect(Object.keys(chromeColors(true))).toEqual(Object.keys(chromeColors(false)));
  });

  it("are applied to an element as custom properties, with the colour scheme", () => {
    const set = new Map<string, string>();
    applyChromeColors({ style: { setProperty: (k: string, v: string) => set.set(k, v) } }, true);
    expect(set.get("--island-fill")).toBe("#232329");
    expect(set.get("color-scheme")).toBe("dark");
  });
});
