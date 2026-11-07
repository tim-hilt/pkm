import { type GeometryBuffers, type GeometryPart, makePart, recordsOf } from "./GeometryBuffers";

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
  };
}
