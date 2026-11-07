import { describe, expect, it } from "vitest";
import { GeometryVerb } from "./GeometryBuffers";
import { fillMeshOfPart, fillMeshOfPath } from "./FillMesh";
import { type Point, fillPart, fillSubpaths, handBuiltBuffers } from "./HandBuiltGeometry";

/**
 * A fill's path becomes triangle fans for the stencil pass plus a covering
 * rectangle. Paths are written by hand so that expected vertices are exact.
 * Meshes are flat lists of x, y pairs.
 */

const flat = (points: Point[]) => points.flat();

function meshOf(part: Parameters<typeof handBuiltBuffers>[0], tolerance = 0.1) {
  const buffers = handBuiltBuffers(part);
  return fillMeshOfPart(buffers.parts.at(0), buffers, tolerance);
}

describe("FillMesh", () => {
  it("makes a square two triangles fanned from its first corner", () => {
    const mesh = meshOf(
      fillPart([
        [10, 20],
        [110, 20],
        [110, 80],
        [10, 80],
      ]),
    )!;

    expect(Array.from(mesh.triangles)).toEqual(
      flat([
        [10, 20],
        [110, 20],
        [110, 80],
        [10, 20],
        [110, 80],
        [10, 80],
      ]),
    );
  });

  it("covers the bounds grown by one unit with two triangles", () => {
    const mesh = meshOf(
      fillPart([
        [10, 20],
        [110, 20],
        [110, 80],
        [10, 80],
      ]),
    )!;

    expect(Array.from(mesh.cover)).toEqual(
      flat([
        [9, 19],
        [111, 19],
        [111, 81],
        [9, 19],
        [111, 81],
        [9, 81],
      ]),
    );
  });

  it("fans every subpath from its own first point", () => {
    const outer: Point[] = [
      [0, 0],
      [100, 0],
      [100, 100],
    ];
    const inner: Point[] = [
      [200, 0],
      [300, 0],
      [300, 100],
    ];

    const mesh = meshOf(fillSubpaths([outer, inner]))!;

    expect(Array.from(mesh.triangles)).toEqual(flat([...outer, ...inner]));
  });

  it("has no mesh for a path without area", () => {
    expect(
      meshOf(
        fillPart([
          [0, 0],
          [10, 10],
        ]),
      ),
      "a line",
    ).toBeUndefined();
    expect(meshOf(fillPart([[5, 5]])), "a point").toBeUndefined();
    expect(meshOf(fillPart([])), "nothing").toBeUndefined();
  });

  it("flattens curves within the tolerance", () => {
    // A quarter disc of radius 100: a cubic arc and the two radii.
    const k = 55.22847498;
    const mesh = fillMeshOfPath(
      [GeometryVerb.move, GeometryVerb.line, GeometryVerb.cubic],
      [0, 0, 100, 0, 100, k, k, 100, 0, 100],
      0.05,
    )!;

    expect(mesh.triangles.length / 2, "an arc needs several chords").toBeGreaterThan(3 * 8);
    // Every vertex lies on the radii or on the circle.
    for (let i = 0; i < mesh.triangles.length; i += 2) {
      const [x, y] = [mesh.triangles[i], mesh.triangles[i + 1]];
      const onRadius = x === 0 || y === 0;
      expect(onRadius || Math.abs(Math.hypot(x, y) - 100) < 0.1, `${x}, ${y}`).toBe(true);
    }
  });
});
