import { type GeometryBuffers, type GeometryPart, dashOf } from "./GeometryBuffers";
import { type PathSink, flattenPart } from "./PathFlattener";

/**
 * Turns a stroked part's paths into straight segments: curves are flattened
 * (see `PathFlattener`) and dashes cut along the flattened path.
 *
 * The segments of `part`, in drawing order, as `x0, y0, x1, y1` quadruples in
 * Scene coordinates.
 *
 * A `move` starts a new subpath without producing a segment; zero-length
 * `line`s are kept, they draw a round dot. For a dashed part only the drawn
 * stretches are returned, split at the path's vertices.
 */
export function strokeSegments(
  part: GeometryPart,
  buffers: GeometryBuffers,
  tolerance: number,
): Float32Array {
  const output = new SegmentWriter(dashOf(part));
  flattenPart(part, buffers, tolerance, output);
  return Float32Array.from(output.segments);
}

/** Collects segments, cutting them into dashes if the stroke is dashed. */
class SegmentWriter implements PathSink {
  readonly segments: number[] = [];
  /** Whether the pattern is in a drawn stretch, and how much of that stretch or gap is left. */
  private drawing = true;
  private remaining: number;

  constructor(private readonly dash: { on: number; off: number } | undefined) {
    this.remaining = dash?.on ?? 0;
  }

  /** The pattern starts over with every subpath. */
  beginSubpath(): void {
    this.drawing = true;
    this.remaining = this.dash?.on ?? 0;
  }

  addLine(x0: number, y0: number, x1: number, y1: number): void {
    const dash = this.dash;
    if (!dash) {
      this.segments.push(x0, y0, x1, y1);
      return;
    }
    const length = Math.hypot(x1 - x0, y1 - y0);
    if (!(length > 0)) return;
    const dx = (x1 - x0) / length;
    const dy = (y1 - y0) / length;
    let travelled = 0;
    while (travelled < length) {
      const left = length - travelled;
      const step = Math.min(left, this.remaining);
      // Land exactly on the end when the stretch reaches it.
      const reachesEnd = step === left;
      if (this.drawing) {
        this.segments.push(
          x0 + dx * travelled,
          y0 + dy * travelled,
          reachesEnd ? x1 : x0 + dx * (travelled + step),
          reachesEnd ? y1 : y0 + dy * (travelled + step),
        );
      }
      travelled = reachesEnd ? length : travelled + step;
      this.remaining -= step;
      if (this.remaining <= 0) {
        this.drawing = !this.drawing;
        this.remaining = this.drawing ? dash.on : dash.off;
      }
    }
  }
}
