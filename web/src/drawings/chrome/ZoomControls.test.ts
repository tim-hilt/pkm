import { describe, expect, it } from "vitest";
import { zoomPercent } from "./ZoomControls";

describe("zoomPercent", () => {
  it("rounds to whole percent", () => {
    expect(zoomPercent(1)).toBe(100);
    expect(zoomPercent(1.2)).toBe(120);
    expect(zoomPercent(0.3333)).toBe(33);
  });
});
