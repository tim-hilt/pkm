import { describe, expect, it } from "vitest";
import { textEdit } from "./QuickAddField";

describe("textEdit", () => {
  it("finds what typing inserted", () => {
    expect(textEdit("Fix ", "Fix p1")).toEqual({ start: 4, end: 4, inserted: "p1" });
  });

  it("finds what deleting removed", () => {
    expect(textEdit("Fix login", "Fix ")).toEqual({ start: 4, end: 9, inserted: "" });
  });

  it("finds what pasting replaced, counting UTF-16 units", () => {
    expect(textEdit("😀 a b", "😀 X b")).toEqual({ start: 3, end: 4, inserted: "X" });
  });

  it("finds nothing in equal texts", () => {
    expect(textEdit("same", "same")).toEqual({ start: 4, end: 4, inserted: "" });
  });
});
