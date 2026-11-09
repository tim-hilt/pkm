import { describe, expect, it } from "vitest";
import { dashOf, elementsInZOrder, fileIdOf, holeOf } from "./GeometryBuffers";
import { handBuiltBuffers, handBuiltBuffersWithText } from "./HandBuiltGeometry";

describe("elementsInZOrder", () => {
  it("yields every Element once, bottom first, with the run of parts that draws it", () => {
    const buffers = handBuiltBuffers(
      { element: 0, verbs: [], coords: [] },
      { element: 0, version: 3, verbs: [], coords: [] },
      { element: 1, version: 7, verbs: [], coords: [] },
    );
    expect([...elementsInZOrder(buffers)]).toEqual([
      { element: 0, version: 1, content: "parts", start: 0, end: 2 },
      { element: 1, version: 7, content: "parts", start: 2, end: 3 },
    ]);
  });

  it("puts lines of text before the part their index names", () => {
    const buffers = handBuiltBuffersWithText(
      [
        { element: 0, verbs: [], coords: [] },
        { element: 1, verbs: [], coords: [] },
      ],
      [
        { element: 10, partIndex: 1, characters: "one" },
        { element: 10, partIndex: 1, characters: "two" },
        { element: 11, partIndex: 2, characters: "end" },
      ],
    );
    expect([...elementsInZOrder(buffers)]).toEqual([
      { element: 0, version: 1, content: "parts", start: 0, end: 1 },
      { element: 10, version: 1, content: "textLines", start: 0, end: 2 },
      { element: 1, version: 1, content: "parts", start: 1, end: 2 },
      { element: 11, version: 1, content: "textLines", start: 2, end: 3 },
    ]);
  });
});

describe("elementsInZOrder with images", () => {
  it("yields an image as one Element, before the part its index names", () => {
    const buffers = handBuiltBuffersWithText(
      [
        { element: 0, verbs: [], coords: [] },
        { element: 1, verbs: [], coords: [] },
      ],
      [],
      [
        { element: 20, partIndex: 1, fileId: "abc" },
        { element: 21, partIndex: 2, fileId: "def", version: 4 },
      ],
    );
    expect([...elementsInZOrder(buffers)]).toEqual([
      { element: 0, version: 1, content: "parts", start: 0, end: 1 },
      { element: 20, version: 1, content: "images", start: 0, end: 1 },
      { element: 1, version: 1, content: "parts", start: 1, end: 2 },
      { element: 21, version: 4, content: "images", start: 1, end: 2 },
    ]);
  });

  it("puts the lower handle first where a line and an image are due at the same part", () => {
    const buffers = handBuiltBuffersWithText(
      [],
      [{ element: 11, partIndex: 0, characters: "x" }],
      [{ element: 10, partIndex: 0 }],
    );
    expect([...elementsInZOrder(buffers)].map((e) => e.content)).toEqual(["images", "textLines"]);
  });

  it("reads the file id of an image", () => {
    const buffers = handBuiltBuffersWithText(
      [],
      [],
      [{ fileId: "first" }, { fileId: "" }, { fileId: "third" }],
    );
    const ids = [0, 1, 2].map((i) => fileIdOf(buffers, buffers.images.at(i)));
    expect(ids).toEqual(["first", "", "third"]);
  });
});

describe("GeometryPart helpers", () => {
  it("has no dash or hole unless the part says so", () => {
    const buffers = handBuiltBuffers({ verbs: [], coords: [] });
    expect(dashOf(buffers.parts.at(0))).toBeUndefined();
    expect(holeOf(buffers.parts.at(0))).toEqual([0, 0, 0, 0]);
  });

  it("reads a dash pattern and a hole", () => {
    const buffers = handBuiltBuffers({
      verbs: [],
      coords: [],
      dashOn: 8,
      dashOff: 10,
      holeX: 1,
      holeY: 2,
      holeWidth: 3,
      holeHeight: 4,
    });
    expect(dashOf(buffers.parts.at(0))).toEqual({ on: 8, off: 10 });
    expect(holeOf(buffers.parts.at(0))).toEqual([1, 2, 4, 6]);
  });
});
