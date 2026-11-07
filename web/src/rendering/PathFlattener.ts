import {
  type GeometryBuffers,
  type GeometryPart,
  GeometryVerb,
  coordsOf,
  verbsOf,
} from "./GeometryBuffers";

/** Receives a flattened path. */
export interface PathSink {
  /** A `move` to `(x, y)` starts a new subpath. */
  beginSubpath(x: number, y: number): void;
  /** A straight stretch of the current subpath. */
  addLine(x0: number, y0: number, x1: number, y1: number): void;
}

/**
 * Walks a part's path and replaces its curves by straight chords. Curves are
 * subdivided on the client (ADR-0008) because only the client knows how large
 * a Scene unit is on screen. Strokes and fills build on it.
 *
 * Feeds `sink` every `move` and every straight stretch, in drawing order; a
 * curve becomes several stretches. Zero-length `line`s are passed on.
 *
 * `tolerance` is the largest allowed distance, in Scene units, between a
 * curve and the stretches replacing it. Callers derive it from the zoom so
 * that the error stays a fraction of a device pixel.
 */
export function flattenPart(
  part: GeometryPart,
  buffers: GeometryBuffers,
  tolerance: number,
  sink: PathSink,
): void {
  flattenPath(verbsOf(buffers, part), coordsOf(buffers, part), tolerance, sink);
}

/** Like `flattenPart` for a path of its own: `verbs` and the operands they consume from `coords`. */
export function flattenPath(
  verbs: ArrayLike<number>,
  coords: ArrayLike<number>,
  tolerance: number,
  sink: PathSink,
): void {
  let next = 0;
  let x = 0;
  let y = 0;

  for (let i = 0; i < verbs.length; i++) {
    switch (verbs[i]) {
      case GeometryVerb.move:
        x = coords[next++];
        y = coords[next++];
        sink.beginSubpath(x, y);
        break;
      case GeometryVerb.line: {
        const endX = coords[next++];
        const endY = coords[next++];
        sink.addLine(x, y, endX, endY);
        x = endX;
        y = endY;
        break;
      }
      case GeometryVerb.cubic: {
        const c1x = coords[next++];
        const c1y = coords[next++];
        const c2x = coords[next++];
        const c2y = coords[next++];
        const endX = coords[next++];
        const endY = coords[next++];
        const steps = stepCount(x, y, c1x, c1y, c2x, c2y, endX, endY, tolerance);
        let previousX = x;
        let previousY = y;
        for (let step = 1; step <= steps; step++) {
          const t = step / steps;
          // Land exactly on the end point instead of on rounding noise.
          const px = step === steps ? endX : cubic(x, c1x, c2x, endX, t);
          const py = step === steps ? endY : cubic(y, c1y, c2y, endY, t);
          sink.addLine(previousX, previousY, px, py);
          previousX = px;
          previousY = py;
        }
        x = endX;
        y = endY;
        break;
      }
      default:
        throw new Error(`unknown verb ${verbs[i]}`);
    }
  }
}

/**
 * Number of chords that keep a cubic within `tolerance`, from the bound on
 * the second differences of its control polygon (Wang's formula for degree 3:
 * n = sqrt(6 * max|second difference| / (8 * tolerance))).
 */
function stepCount(
  p0x: number,
  p0y: number,
  p1x: number,
  p1y: number,
  p2x: number,
  p2y: number,
  p3x: number,
  p3y: number,
  tolerance: number,
): number {
  const d1x = p0x - 2 * p1x + p2x;
  const d1y = p0y - 2 * p1y + p2y;
  const d2x = p1x - 2 * p2x + p3x;
  const d2y = p1y - 2 * p2y + p3y;
  const longest = Math.sqrt(Math.max(d1x * d1x + d1y * d1y, d2x * d2x + d2y * d2y));
  const steps = Math.ceil(Math.sqrt((0.75 * longest) / Math.max(tolerance, Number.MIN_VALUE)));
  return Number.isFinite(steps) ? Math.min(Math.max(steps, 1), 1000) : 1;
}

function cubic(p0: number, p1: number, p2: number, p3: number, t: number): number {
  const u = 1 - t;
  return u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
}
