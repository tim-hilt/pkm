import { describe, expect, it } from "vitest";
import { elementsInZOrder, holeOf, dashOf } from "./GeometryBuffers";
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
