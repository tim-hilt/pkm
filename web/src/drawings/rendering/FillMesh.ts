import { type GeometryBuffers, type GeometryPart } from "./GeometryBuffers";
import { type PathSink, flattenPart, flattenPath } from "./PathFlattener";

/**
 * A fill part prepared for stencil-then-cover (ADR-0002), a port of the Swift
 * client's `FillMesh`: triangles that the GPU rasterises into the stencil
 * buffer to find out which pixels the path encloses, and a rectangle that then
 * paints exactly those. Vertices are x, y pairs in Scene coordinates.
 */
export interface FillMesh {
  /**
   * Vertices of the fan triangles, three per triangle. Every subpath is
   * closed implicitly and fanned out from its first point, so the signed
   * triangles add up to the path's winding number at every point, whatever
   * the shape: concave, with holes, or crossing itself.
   */
  triangles: Float32Array;
  /** Two triangles covering everything the fill can touch. */
  cover: Float32Array;
}

/** The mesh of `part`, or undefined if its path encloses no area. */
export function fillMeshOfPart(
  part: GeometryPart,
  buffers: GeometryBuffers,
  tolerance: number,
): FillMesh | undefined {
  const builder = new MeshBuilder();
  flattenPart(part, buffers, tolerance, builder);
  return builder.finish();
}

/**
 * The mesh of the path of `verbs` and `coords`, in the format of the core's
 * geometry, or undefined if it encloses no area.
 */
export function fillMeshOfPath(
  verbs: ArrayLike<number>,
  coords: ArrayLike<number>,
  tolerance: number,
): FillMesh | undefined {
  const builder = new MeshBuilder();
  flattenPath(verbs, coords, tolerance, builder);
  return builder.finish();
}

class MeshBuilder implements PathSink {
  private readonly triangles: number[] = [];
  private lowerX = Infinity;
  private lowerY = Infinity;
  private upperX = -Infinity;
  private upperY = -Infinity;
  /** First and latest point of the subpath, and how many it has. */
  private anchorX = 0;
  private anchorY = 0;
  private lastX = 0;
  private lastY = 0;
  private pointCount = 0;

  beginSubpath(x: number, y: number): void {
    this.anchorX = this.lastX = x;
    this.anchorY = this.lastY = y;
    this.pointCount = 1;
    this.include(x, y);
  }

  addLine(_x0: number, _y0: number, x1: number, y1: number): void {
    if (this.pointCount >= 2) {
      this.triangles.push(this.anchorX, this.anchorY, this.lastX, this.lastY, x1, y1);
    }
    this.lastX = x1;
    this.lastY = y1;
    this.pointCount++;
    this.include(x1, y1);
  }

  private include(x: number, y: number): void {
    this.lowerX = Math.min(this.lowerX, x);
    this.lowerY = Math.min(this.lowerY, y);
    this.upperX = Math.max(this.upperX, x);
    this.upperY = Math.max(this.upperY, y);
  }

  /** The cover is a little larger than the path, so that no pixel the path reaches into lies outside it. */
  finish(): FillMesh | undefined {
    if (this.triangles.length === 0) return undefined;
    const [x0, y0, x1, y1] = [this.lowerX - 1, this.lowerY - 1, this.upperX + 1, this.upperY + 1];
    return {
      triangles: Float32Array.from(this.triangles),
      cover: Float32Array.of(x0, y0, x1, y0, x1, y1, x0, y0, x1, y1, x0, y1),
    };
  }
}
