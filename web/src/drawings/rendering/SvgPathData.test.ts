import { describe, expect, it } from "vitest";
import { parseSvgPath } from "./SvgPathData";

/** The ops of a path as readable text, e.g. `move 10 20`. */
function ops(data: string): string[] {
  const path = parseSvgPath(data);
  let at = 0;
  return path.verbs.map((verb) => {
    const count = verb === 2 ? 6 : 2;
    const text = `${["move", "line", "cubic"][verb]} ${path.coords.slice(at, at + count).join(" ")}`;
    at += count;
    return text;
  });
}

describe("parseSvgPath", () => {
  it("goes where absolute commands say", () => {
    expect(ops("M10 20L30 40")).toEqual(["move 10 20", "line 30 40"]);
  });

  it("starts relative commands from the current point", () => {
    expect(ops("M10 20l5 5l-5 0")).toEqual(["move 10 20", "line 15 25", "line 10 25"]);
  });

  it("keeps the other coordinate of horizontal and vertical lines", () => {
    expect(ops("M10 20H30V50h-5v-10")).toEqual([
      "move 10 20",
      "line 30 20",
      "line 30 50",
      "line 25 50",
      "line 25 40",
    ]);
  });

  it("lets numbers run together at a sign or a second dot", () => {
    expect(ops("M1-2L.5.25")).toEqual(["move 1 -2", "line 0.5 0.25"]);
  });

  it("repeats a command for further pairs, and a move continues with lines", () => {
    expect(ops("M0 0 10 0 10 10")).toEqual(["move 0 0", "line 10 0", "line 10 10"]);
    expect(ops("M0 0l1 1 1 1")).toEqual(["move 0 0", "line 1 1", "line 2 2"]);
  });

  it("takes two control points and an end for a cubic, and reflects for the smooth form", () => {
    expect(ops("M0 0c1 2 3 4 5 6")).toEqual(["move 0 0", "cubic 1 2 3 4 5 6"]);
    expect(ops("M0 0C1 2 3 4 5 6S9 9 10 10")).toEqual([
      "move 0 0",
      "cubic 1 2 3 4 5 6",
      "cubic 7 8 9 9 10 10",
    ]);
  });

  it("returns to the start of the subpath on close", () => {
    expect(ops("M10 10l5 0 0 5zm1 1l2 0")).toEqual([
      "move 10 10",
      "line 15 10",
      "line 15 15",
      "move 11 11",
      "line 13 11",
    ]);
    expect(ops("M10 10l5 0zl0 5")).toEqual([
      "move 10 10",
      "line 15 10",
      "move 10 10",
      "line 10 15",
    ]);
  });

  it("parses as far as it can", () => {
    expect(ops("M1 2L3 4A5 5 0 0 1 9 9L7 7")).toEqual(["move 1 2", "line 3 4"]);
    expect(ops("")).toEqual([]);
  });
});
