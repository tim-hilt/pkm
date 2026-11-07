import {
  type GeometryBuffers,
  type GeometryPart,
  type GeometryTextLine,
  type GeometrySource,
  GeometryFillRule,
  fillKind,
  makePart,
  recordsOf,
  strokeKind,
} from "./GeometryBuffers";

/** A point as `[x, y]`. */
export type Point = readonly [number, number];

/** One part of hand-built geometry: its path and the part's style. */
export interface HandBuiltPart extends Partial<GeometryPart> {
  verbs: number[];
  coords: number[];
}

/**
 * Geometry written by hand, for tests: the expected results are exact
 * because the paths are. Parts' verbs and coordinates are laid out one after
 * the other, as the core does.
 */
export function handBuiltBuffers(...parts: HandBuiltPart[]): GeometryBuffers {
  return handBuiltBuffersWithText(parts, []);
}

/** A line of hand-built text: its record's fields (the rest zero) and its characters. */
export interface HandBuiltTextLine extends Partial<GeometryTextLine> {
  characters: string;
}

/** Hand-built parts and lines of text; a line comes before the part its `partIndex` names. */
export function handBuiltBuffersWithText(
  parts: HandBuiltPart[],
  lines: HandBuiltTextLine[],
): GeometryBuffers {
  const text: number[] = [];
  const textLines: GeometryTextLine[] = lines.map(({ characters, ...fields }, index) => {
    const bytes = new TextEncoder().encode(characters);
    const record: GeometryTextLine = {
      element: 100 + index,
      version: 1,
      partIndex: 0,
      color: 0x1e1e1eff,
      opacity: 1,
      fontSize: 20,
      fontFamily: 5,
      align: 0,
      x: 0,
      y: 0,
      angle: 0,
      ...fields,
      textStart: text.length,
      textLength: bytes.length,
    };
    text.push(...bytes);
    return record;
  });
  const verbs: number[] = [];
  const coords: number[] = [];
  const records: GeometryPart[] = parts.map((part, element) => {
    const record = makePart({
      element,
      ...part,
      verbStart: verbs.length,
      verbCount: part.verbs.length,
      coordStart: coords.length,
    });
    verbs.push(...part.verbs);
    coords.push(...part.coords);
    return record;
  });
  return {
    parts: recordsOf(records),
    verbs: Uint8Array.from(verbs),
    coords: Float32Array.from(coords),
    textLines: recordsOf(textLines),
    text: Uint8Array.from(text),
  };
}

/** Hand-built parts as geometry the renderer can draw without the core: every part is its own Element. */
export function handBuiltGeometry(...parts: HandBuiltPart[]): GeometrySource {
  const buffers = handBuiltBuffers(...parts);
  return { generation: () => 0, withBuffers: (body) => body(buffers) };
}

/** The path through each list of points: a `move` to its first point, then `line`s. */
export function polylines(...subpaths: Point[][]): Pick<HandBuiltPart, "verbs" | "coords"> {
  const verbs: number[] = [];
  const coords: number[] = [];
  for (const points of subpaths) {
    points.forEach(([x, y], index) => {
      verbs.push(index === 0 ? 0 : 1);
      coords.push(x, y);
    });
  }
  return { verbs, coords };
}

/** A fill through `points`, closed implicitly. */
export function fillPart(points: Point[], style: Partial<GeometryPart> = {}): HandBuiltPart {
  return fillSubpaths([points], style);
}

/** Several subpaths in one fill part. */
export function fillSubpaths(
  subpaths: Point[][],
  style: Partial<GeometryPart> = {},
): HandBuiltPart {
  return {
    kind: fillKind,
    color: 0xa5d8ffff,
    fillRule: GeometryFillRule.nonZero,
    width: 0,
    ...style,
    ...polylines(...subpaths),
  };
}

/** A stroked polyline through `points`. */
export function strokePart(points: Point[], style: Partial<GeometryPart> = {}): HandBuiltPart {
  return { kind: strokeKind, width: 2, color: 0x1e1e1eff, ...style, ...polylines(points) };
}
