import { describe, expect, it } from "vitest";
import { DimensionsFade } from "./DimensionsFade";

describe("the fade of the Dimensions", () => {
  it("shows nothing until the core gives an output", () => {
    const fade = new DimensionsFade<string>();
    expect(fade.visible(0)).toBeUndefined();
    fade.update(undefined, 0);
    expect(fade.visible(0)).toBeUndefined();
    expect(fade.isFading(0)).toBe(false);
  });

  it("shows an output at full strength at once", () => {
    const fade = new DimensionsFade<string>();
    fade.update("11.40", 100);
    expect(fade.visible(100)).toEqual({ dimensions: "11.40", strength: 1 });
    expect(fade.visible(5000)).toEqual({ dimensions: "11.40", strength: 1 });
    expect(fade.isFading(5000)).toBe(false);
  });

  it("fades the last output out over 0.4 s once the core's goes away", () => {
    const fade = new DimensionsFade<string>();
    fade.update("a", 0);
    fade.update(undefined, 1000);
    expect(fade.isFading(1000)).toBe(true);
    expect(fade.visible(1000)?.strength).toBe(1);
    expect(fade.visible(1100)?.strength).toBeCloseTo(0.75);
    expect(fade.visible(1399)?.strength).toBeGreaterThan(0);
    expect(fade.visible(1400)).toBeUndefined();
    expect(fade.isFading(1400)).toBe(false);
  });

  it("does not restart the fade on further empty outputs", () => {
    const fade = new DimensionsFade<string>();
    fade.update("a", 0);
    fade.update(undefined, 1000);
    fade.update(undefined, 1300);
    expect(fade.visible(1300)?.strength).toBeCloseTo(0.25);
  });

  it("replaces a fading output by a new one at full strength", () => {
    const fade = new DimensionsFade<string>();
    fade.update("a", 0);
    fade.update(undefined, 1000);
    fade.update("b", 1200);
    expect(fade.visible(1200)).toEqual({ dimensions: "b", strength: 1 });
    expect(fade.isFading(1200)).toBe(false);
  });

  it("forgets everything on reset, without fading", () => {
    const fade = new DimensionsFade<string>();
    fade.update("a", 0);
    fade.reset();
    expect(fade.visible(0)).toBeUndefined();
  });
});
