import { describe, expect, it } from "vitest";
import { Cursor } from "../../core/Core";
import { cssCursor } from "./CursorStyle";

describe("cssCursor", () => {
  it("gives every core cursor a CSS cursor", () => {
    const all = Object.values(Cursor).filter((v): v is Cursor => typeof v === "number");
    expect(all.length).toBe(12);
    for (const cursor of all) expect(cssCursor(cursor)).not.toBe("");
  });

  it("maps the resize cursors to their axis", () => {
    expect(cssCursor(Cursor.ResizeNorthSouth)).toBe("ns-resize");
    expect(cssCursor(Cursor.ResizeNorthWestSouthEast)).toBe("nwse-resize");
    expect(cssCursor(Cursor.Grabbing)).toBe("grabbing");
  });

  it("draws the eraser as a disc image", () => {
    expect(cssCursor(Cursor.Eraser)).toMatch(/^url\("data:image\/svg\+xml,.*"\) 7 7, crosshair$/);
  });
});
