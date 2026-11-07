import { describe, expect, it } from "vitest";
import { type HandBuiltPart, handBuiltBuffers } from "./HandBuiltGeometry";
import { strokeSegments } from "./StrokeFlattener";

/**
 * Curves are flattened on the client; expected values come from the circle the
 * rounded rectangle's corners approximate, not from the flattener's maths. The
 * buffers are written by hand so that the expected geometry is exact.
 */

type Segment = [number, number, number, number];

function flatten(part: HandBuiltPart, tolerance: number): Segment[] {
  const buffers = handBuiltBuffers(part);
  const flat = strokeSegments(buffers.parts.at(0), buffers, tolerance);
  const segments: Segment[] = [];
  for (let i = 0; i < flat.length; i += 4) {
    segments.push([flat[i], flat[i + 1], flat[i + 2], flat[i + 3]]);
  }
  return segments;
}

/**
 * Flattens a rectangle (50,100)-(330,280) with corner radius 32, built the way
 * the core describes rounded shapes: lines and one cubic per corner.
 */
function flattenedRectangle(tolerance: number): Segment[] {
  const [x, y, right, bottom, r] = [50, 100, 330, 280, 32];
  const k = r * 0.5522848; // control-point distance of a quarter circle
  return flatten(
    {
      verbs: [0, 1, 2, 1, 2, 1, 2, 1, 2],
      coords: [
        x + r,
        y,
        right - r,
        y,
        right - r + k,
        y,
        right,
        y + r - k,
        right,
        y + r,
        right,
        bottom - r,
        right,
        bottom - r + k,
        right - r + k,
        bottom,
        right - r,
        bottom,
        x + r,
        bottom,
        x + r - k,
        bottom,
        x,
        bottom - r + k,
        x,
        bottom - r,
        x,
        y + r,
        x,
        y + r - k,
        x + r - k,
        y,
        x + r,
        y,
      ],
    },
    tolerance,
  );
}

/** Flattens the path made of `verbs` (0 move, 1 line) drawn dashed 8 on, 10 off. */
function dashed(verbs: number[], coords: number[], on = 8, off = 10): Segment[] {
  return flatten({ verbs, coords, dashOn: on, dashOff: off }, 0.1);
}

describe("StrokeFlattener", () => {
  it("keeps straight edges as single segments", () => {
    // The top edge is the first line after the move.
    expect(flattenedRectangle(0.25)[0]).toEqual([82, 100, 298, 100]);
  });

  it("stays within tolerance of the corner", () => {
    const tolerance = 0.05;
    const corner = [298, 132]; // centre of the top-right corner arc
    const arc = flattenedRectangle(tolerance).filter(
      ([x0, y0, x1, y1]) => x0 >= 298 && x1 <= 330 && y0 <= 132 && y1 <= 132,
    );
    expect(arc.length).toBeGreaterThan(3);
    for (const [x0, y0, x1, y1] of arc) {
      const distance = Math.hypot((x0 + x1) / 2 - corner[0], (y0 + y1) / 2 - corner[1]);
      // Chord midpoints lie just inside the circle; allow the cubic's own
      // 0.03% radius error on top of the requested tolerance.
      expect(distance).toBeLessThanOrEqual(32 + 0.02);
      expect(distance).toBeGreaterThanOrEqual(32 - tolerance - 0.02);
    }
  });

  it("yields more segments for a finer tolerance", () => {
    expect(flattenedRectangle(0.01).length).toBeGreaterThan(flattenedRectangle(1).length);
  });

  it("draws a dashed line in stretches of the on length separated by the off length", () => {
    // 0-8 on, 8-18 off, 18-26 on, ... the last stretch 90-98 ends before the line does.
    expect(dashed([0, 1], [0, 0, 100, 0])).toEqual([
      [0, 0, 8, 0],
      [18, 0, 26, 0],
      [36, 0, 44, 0],
      [54, 0, 62, 0],
      [72, 0, 80, 0],
      [90, 0, 98, 0],
    ]);
  });

  it("runs a dash on around a corner", () => {
    // 5 along the first leg, then 3 more on the second: 8 in all.
    expect(dashed([0, 1, 1], [0, 0, 5, 0, 5, 10])).toEqual([
      [0, 0, 5, 0],
      [5, 0, 5, 3],
    ]);
  });

  it("runs a gap on around a corner too", () => {
    // The first stretch ends at 8 on a 10 long leg, the gap then ends at 18:
    // 8 into the second leg.
    expect(dashed([0, 1, 1], [0, 0, 10, 0, 10, 100]).slice(0, 2)).toEqual([
      [0, 0, 8, 0],
      [10, 8, 10, 16],
    ]);
  });

  it("starts the dash pattern again at every move", () => {
    expect(dashed([0, 1, 0, 1], [0, 0, 10, 0, 0, 5, 10, 5])).toEqual([
      [0, 0, 8, 0],
      [0, 5, 8, 5],
    ]);
  });

  it("makes a dotted line of short dots far apart", () => {
    expect(dashed([0, 1], [0, 0, 30, 0], 1.5, 8)).toEqual([
      [0, 0, 1.5, 0],
      [9.5, 0, 11, 0],
      [19, 0, 20.5, 0],
      [28.5, 0, 30, 0],
    ]);
  });

  it("cuts a dashed curve along its length", () => {
    // The same cubic as the first corner of the rounded rectangle, a quarter
    // circle of radius 32 of length 16 * pi = 50.27: a dash of 8 and a gap of
    // 10 give three dashes (0-8, 18-26, 36-44).
    const k = 32 * 0.5522848;
    const segments = dashed([0, 2], [0, 32, 0, 32 - k, 32 - k, 0, 32, 0]);
    let length = 0;
    let dashes = 0;
    let previousEnd: [number, number] | undefined;
    for (const [x0, y0, x1, y1] of segments) {
      length += Math.hypot(x1 - x0, y1 - y0);
      if (previousEnd?.[0] !== x0 || previousEnd[1] !== y0) dashes++;
      previousEnd = [x1, y1];
    }
    expect(dashes).toBe(3);
    // Chords undercut the arc a little: 3 * 8 = 24, within a percent.
    expect(Math.abs(length - 24)).toBeLessThan(0.3);
  });

  it("outlines continuously and closed", () => {
    const segments = flattenedRectangle(0.1);
    segments.slice(1).forEach((next, i) => {
      const previous = segments[i];
      expect([previous[2], previous[3]]).toEqual([next[0], next[1]]);
    });
    const last = segments[segments.length - 1];
    expect([segments[0][0], segments[0][1]]).toEqual([last[2], last[3]]);
  });
});
