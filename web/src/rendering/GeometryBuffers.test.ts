import { describe, expect, it } from "vitest";
import { elementsInZOrder, holeOf, dashOf } from "./GeometryBuffers";
import { handBuiltBuffers } from "./HandBuiltGeometry";

describe("elementsInZOrder", () => {
  it("yields every Element once, bottom first, with the run of parts that draws it", () => {
    const buffers = handBuiltBuffers(
      { element: 0, verbs: [], coords: [] },
      { element: 0, version: 3, verbs: [], coords: [] },
      { element: 1, version: 7, verbs: [], coords: [] },
    );
    expect([...elementsInZOrder(buffers)]).toEqual([
      { element: 0, version: 1, start: 0, end: 2 },
      { element: 1, version: 7, start: 2, end: 3 },
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
