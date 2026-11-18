import { describe, expect, it } from "vitest";
import { textEdit } from "./TextCaret";

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

  it("never splits a character made of two UTF-16 units", () => {
    // 😀 and 😃 share their first unit; typing one before the other.
    expect(textEdit("😀", "😃😀")).toEqual({ start: 0, end: 0, inserted: "😃" });
    expect(textEdit("a😀", "a😃")).toEqual({ start: 1, end: 3, inserted: "😃" });
  });

  it("finds nothing in equal texts", () => {
    expect(textEdit("same", "same")).toEqual({ start: 4, end: 4, inserted: "" });
  });

  it("finds a line break typed between lines", () => {
    expect(textEdit("one\ntwo", "one\n\ntwo")).toEqual({ start: 4, end: 4, inserted: "\n" });
  });
});
