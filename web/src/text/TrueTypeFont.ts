import { GeometryVerb } from "../rendering/GeometryBuffers";

/**
 * The outline of one glyph in the font's design units, y pointing down like
 * the Scene's. `verbs` and `coords` are in the format of the core's geometry;
 * every contour is closed implicitly, as fills treat their paths.
 */
export interface GlyphOutline {
  verbs: number[];
  coords: number[];
}

/**
 * A TrueType font file, read for what text needs: character to glyph
 * (`cmap`, format 4), advances (`hmtx`), pair kerning (`GPOS` pair
 * adjustment, the `kern` feature) and outlines (`glyf`). Replaces CoreText,
 * which the Swift client uses.
 */
export class TrueTypeFont {
  readonly unitsPerEm: number;
  readonly glyphCount: number;
  private readonly view: DataView;
  private readonly tables = new Map<string, number>();
  private readonly longLocations: boolean;
  private readonly metricCount: number;
  private readonly segments: CmapSegments;
  private readonly kernLookups: number[][];

  /** Throws if `bytes` is not a TrueType font this reader understands. */
  constructor(bytes: Uint8Array) {
    this.view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const tableCount = this.view.getUint16(4);
    for (let i = 0; i < tableCount; i++) {
      const record = 12 + 16 * i;
      const tag = String.fromCharCode(...bytes.subarray(record, record + 4));
      this.tables.set(tag, this.view.getUint32(record + 8));
    }
    const head = this.table("head");
    this.unitsPerEm = this.view.getUint16(head + 18);
    this.longLocations = this.view.getInt16(head + 50) !== 0;
    this.glyphCount = this.view.getUint16(this.table("maxp") + 4);
    this.metricCount = this.view.getUint16(this.table("hhea") + 34);
    this.segments = this.readCmap();
    this.kernLookups = this.tables.has("GPOS") ? this.readKernLookups() : [];
  }

  /** The glyph for `codePoint`; 0, the font's missing glyph (`.notdef`), if it has none. */
  glyphIndex(codePoint: number): number {
    const { view } = this;
    const { start, end, delta, rangeOffset, rangeOffsetsAt } = this.segments;
    for (let i = 0; i < start.length; i++) {
      if (codePoint > end[i]) continue;
      if (codePoint < start[i]) return 0;
      if (rangeOffset[i] === 0) return (codePoint + delta[i]) & 0xffff;
      const at = rangeOffsetsAt + 2 * i + rangeOffset[i] + 2 * (codePoint - start[i]);
      const glyph = view.getUint16(at);
      return glyph === 0 ? 0 : (glyph + delta[i]) & 0xffff;
    }
    return 0;
  }

  /** The advance width of `glyph` in design units. */
  advance(glyph: number): number {
    const hmtx = this.table("hmtx");
    return this.view.getUint16(hmtx + 4 * Math.min(glyph, this.metricCount - 1));
  }

  /** What to add to the advance of `left` when `right` follows it, in design units. */
  kerning(left: number, right: number): number {
    for (const subtables of this.kernLookups) {
      for (const subtable of subtables) {
        const value = this.pairAdjustment(subtable, left, right);
        if (value !== undefined) return value;
      }
    }
    return 0;
  }

  /** The outline of `glyph`: quadratic curves raised to cubics, y pointing down. */
  outline(glyph: number): GlyphOutline {
    const outline: GlyphOutline = { verbs: [], coords: [] };
    this.addGlyph(glyph, [1, 0, 0, 1, 0, 0], outline, 0);
    return outline;
  }

  private table(tag: string): number {
    const offset = this.tables.get(tag);
    if (offset === undefined) throw new Error(`the font has no ${tag} table`);
    return offset;
  }

  // MARK: cmap

  private readCmap(): CmapSegments {
    const { view } = this;
    const cmap = this.table("cmap");
    let subtable: number | undefined;
    for (let i = 0; i < view.getUint16(cmap + 2); i++) {
      const record = cmap + 4 + 8 * i;
      const platform = view.getUint16(record);
      const encoding = view.getUint16(record + 2);
      const at = cmap + view.getUint32(record + 4);
      // Unicode BMP: Windows (3, 1) or the Unicode platform.
      if (view.getUint16(at) === 4 && ((platform === 3 && encoding === 1) || platform === 0)) {
        subtable = at;
        break;
      }
    }
    if (subtable === undefined) throw new Error("the font has no Unicode cmap of format 4");
    const count = view.getUint16(subtable + 6) / 2;
    const ends = subtable + 14;
    const starts = ends + 2 * count + 2;
    const deltas = starts + 2 * count;
    const offsets = deltas + 2 * count;
    const read = (from: number, signed = false) =>
      Array.from({ length: count }, (_, i) =>
        signed ? view.getInt16(from + 2 * i) : view.getUint16(from + 2 * i),
      );
    return {
      end: read(ends),
      start: read(starts),
      delta: read(deltas, true),
      rangeOffset: read(offsets),
      rangeOffsetsAt: offsets,
    };
  }

  // MARK: kerning

  /** For each lookup of the `kern` feature, in order, the offsets of its pair adjustment subtables. */
  private readKernLookups(): number[][] {
    const { view } = this;
    const gpos = this.table("GPOS");
    const features = gpos + view.getUint16(gpos + 6);
    const lookups = gpos + view.getUint16(gpos + 8);
    const wanted = new Set<number>();
    for (let i = 0; i < view.getUint16(features); i++) {
      const record = features + 2 + 6 * i;
      const tag = String.fromCharCode(...[0, 1, 2, 3].map((k) => view.getUint8(record + k)));
      if (tag !== "kern") continue;
      const feature = features + view.getUint16(record + 4);
      for (let j = 0; j < view.getUint16(feature + 2); j++) {
        wanted.add(view.getUint16(feature + 4 + 2 * j));
      }
    }
    return [...wanted]
      .sort((a, b) => a - b)
      .map((index) => {
        const lookup = lookups + view.getUint16(lookups + 2 + 2 * index);
        let type = view.getUint16(lookup);
        const subtables: number[] = [];
        for (let k = 0; k < view.getUint16(lookup + 4); k++) {
          let subtable = lookup + view.getUint16(lookup + 6 + 2 * k);
          if (type === 9) {
            // Extension: the real subtable and its type are behind an offset.
            type = view.getUint16(subtable + 2);
            subtable += view.getUint32(subtable + 4);
          }
          if (type === 2) subtables.push(subtable);
        }
        return subtables;
      });
  }

  /** The x advance change `subtable` (pair adjustment, format 1 or 2) asks for the pair; undefined if it does not cover it. */
  private pairAdjustment(subtable: number, left: number, right: number): number | undefined {
    const { view } = this;
    const format = view.getUint16(subtable);
    const coverage = this.coverageIndex(subtable + view.getUint16(subtable + 2), left);
    if (coverage === undefined) return undefined;
    const format1 = view.getUint16(subtable + 4);
    const format2 = view.getUint16(subtable + 6);
    const size1 = valueRecordSize(format1);
    const size2 = valueRecordSize(format2);
    const xAdvance = (record: number, valueFormat: number) =>
      valueFormat & xAdvanceFlag ? view.getInt16(record + 2 * popCount(valueFormat & 3)) : 0;
    if (format === 1) {
      const set = subtable + view.getUint16(subtable + 10 + 2 * coverage);
      const recordSize = 2 + size1 + size2;
      for (let i = 0; i < view.getUint16(set); i++) {
        const record = set + 2 + i * recordSize;
        if (view.getUint16(record) === right) return xAdvance(record + 2, format1);
      }
      return undefined;
    }
    if (format === 2) {
      const class1 = this.classOf(subtable + view.getUint16(subtable + 8), left);
      const class2 = this.classOf(subtable + view.getUint16(subtable + 10), right);
      const class2Count = view.getUint16(subtable + 14);
      const record = subtable + 16 + (class1 * class2Count + class2) * (size1 + size2);
      return xAdvance(record, format1);
    }
    return undefined;
  }

  /** The position of `glyph` in the coverage table at `table`; undefined if it is not covered. */
  private coverageIndex(table: number, glyph: number): number | undefined {
    const { view } = this;
    const count = view.getUint16(table + 2);
    if (view.getUint16(table) === 1) {
      for (let i = 0; i < count; i++) if (view.getUint16(table + 4 + 2 * i) === glyph) return i;
      return undefined;
    }
    for (let i = 0; i < count; i++) {
      const range = table + 4 + 6 * i;
      const first = view.getUint16(range);
      if (glyph >= first && glyph <= view.getUint16(range + 2)) {
        return view.getUint16(range + 4) + glyph - first;
      }
    }
    return undefined;
  }

  /** The class of `glyph` in the class definition at `table`; 0 if it has none. */
  private classOf(table: number, glyph: number): number {
    const { view } = this;
    if (view.getUint16(table) === 1) {
      const first = view.getUint16(table + 2);
      const index = glyph - first;
      return index >= 0 && index < view.getUint16(table + 4)
        ? view.getUint16(table + 6 + 2 * index)
        : 0;
    }
    for (let i = 0; i < view.getUint16(table + 2); i++) {
      const range = table + 4 + 6 * i;
      if (glyph >= view.getUint16(range) && glyph <= view.getUint16(range + 2)) {
        return view.getUint16(range + 4);
      }
    }
    return 0;
  }

  // MARK: glyf

  /** Adds `glyph` to `outline`, mapped by the matrix `[a, b, c, d, dx, dy]` (x' = a·x + c·y + dx). */
  private addGlyph(
    glyph: number,
    matrix: readonly number[],
    outline: GlyphOutline,
    depth: number,
  ): void {
    const { view } = this;
    if (glyph >= this.glyphCount || depth > 8) return;
    const loca = this.table("loca");
    const [from, to] = this.longLocations
      ? [view.getUint32(loca + 4 * glyph), view.getUint32(loca + 4 * glyph + 4)]
      : [2 * view.getUint16(loca + 2 * glyph), 2 * view.getUint16(loca + 2 * glyph + 2)];
    if (from === to) return; // No outline, like a space.
    const at = this.table("glyf") + from;
    const contourCount = view.getInt16(at);
    if (contourCount >= 0) this.addSimpleGlyph(at, contourCount, matrix, outline);
    else this.addCompositeGlyph(at + 10, matrix, outline, depth);
  }

  private addSimpleGlyph(
    at: number,
    contourCount: number,
    matrix: readonly number[],
    outline: GlyphOutline,
  ): void {
    const { view } = this;
    const ends = Array.from({ length: contourCount }, (_, i) => view.getUint16(at + 10 + 2 * i));
    const pointCount = contourCount === 0 ? 0 : ends[contourCount - 1] + 1;
    let cursor = at + 10 + 2 * contourCount;
    cursor += 2 + view.getUint16(cursor); // Skip the instructions.

    const flags: number[] = [];
    while (flags.length < pointCount) {
      const flag = view.getUint8(cursor++);
      flags.push(flag);
      if (flag & 8) for (let n = view.getUint8(cursor++); n > 0; n--) flags.push(flag);
    }
    const axis = (shortFlag: number, sameFlag: number): number[] => {
      const values: number[] = [];
      let value = 0;
      for (const flag of flags) {
        if (flag & shortFlag) {
          const magnitude = view.getUint8(cursor++);
          value += flag & sameFlag ? magnitude : -magnitude;
        } else if (!(flag & sameFlag)) {
          value += view.getInt16(cursor);
          cursor += 2;
        }
        values.push(value);
      }
      return values;
    };
    const xs = axis(2, 16);
    const ys = axis(4, 32);

    let first = 0;
    for (const end of ends) {
      const points = [];
      for (let i = first; i <= end; i++) {
        const [x, y] = transform(matrix, xs[i], ys[i]);
        points.push({ x, y: -y, onCurve: (flags[i] & 1) !== 0 });
      }
      addContour(points, outline);
      first = end + 1;
    }
  }

  private addCompositeGlyph(
    at: number,
    matrix: readonly number[],
    outline: GlyphOutline,
    depth: number,
  ): void {
    const { view } = this;
    let cursor = at;
    let flags: number;
    do {
      flags = view.getUint16(cursor);
      const glyph = view.getUint16(cursor + 2);
      cursor += 4;
      let dx: number;
      let dy: number;
      if (flags & 1) {
        dx = flags & 2 ? view.getInt16(cursor) : 0;
        dy = flags & 2 ? view.getInt16(cursor + 2) : 0;
        cursor += 4;
      } else {
        dx = flags & 2 ? view.getInt8(cursor) : 0;
        dy = flags & 2 ? view.getInt8(cursor + 1) : 0;
        cursor += 2;
      }
      const f2dot14 = (offset: number) => view.getInt16(cursor + offset) / 16384;
      let [a, b, c, d] = [1, 0, 0, 1];
      if (flags & 8) {
        a = d = f2dot14(0);
        cursor += 2;
      } else if (flags & 0x40) {
        a = f2dot14(0);
        d = f2dot14(2);
        cursor += 4;
      } else if (flags & 0x80) {
        [a, b, c, d] = [f2dot14(0), f2dot14(2), f2dot14(4), f2dot14(6)];
        cursor += 8;
      }
      // Each component point is mapped by its own matrix and offset, then by the parent's.
      const [pa, pb, pc, pd] = matrix;
      const [tx, ty] = transform(matrix, dx, dy);
      this.addGlyph(
        glyph,
        [pa * a + pc * b, pb * a + pd * b, pa * c + pc * d, pb * c + pd * d, tx, ty],
        outline,
        depth + 1,
      );
    } while (flags & 0x20);
  }
}

interface CmapSegments {
  end: number[];
  start: number[];
  delta: number[];
  rangeOffset: number[];
  /** Where the range offsets table starts; the offsets are relative to their own position. */
  rangeOffsetsAt: number;
}

const xAdvanceFlag = 4;

function popCount(value: number): number {
  let count = 0;
  for (let v = value; v; v >>= 1) count += v & 1;
  return count;
}

/** Bytes of a GPOS value record with the fields `valueFormat` says. */
function valueRecordSize(valueFormat: number): number {
  return 2 * popCount(valueFormat);
}

/** `(x, y)` mapped by `[a, b, c, d, dx, dy]`. */
function transform(matrix: readonly number[], x: number, y: number): [number, number] {
  const [a, b, c, d, dx, dy] = matrix;
  return [a * x + c * y + dx, b * x + d * y + dy];
}

interface ContourPoint {
  x: number;
  y: number;
  onCurve: boolean;
}

/**
 * Appends the closed contour through `points` (TrueType's on- and off-curve
 * points, where two off-curve points in a row imply an on-curve one between
 * them) as a move, lines and cubics.
 */
function addContour(points: ContourPoint[], outline: GlyphOutline): void {
  if (points.length < 2) return;
  // Start on an on-curve point; if there is none, on the middle of two off-curve ones.
  let start = points.findIndex((p) => p.onCurve);
  let ring = points;
  if (start < 0) {
    const [a, b] = [points[points.length - 1], points[0]];
    ring = [{ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2, onCurve: true }, ...points];
    start = 0;
  }
  const ordered = [...ring.slice(start), ...ring.slice(0, start)];
  const { verbs, coords } = outline;
  let current = ordered[0];
  verbs.push(GeometryVerb.move);
  coords.push(current.x, current.y);

  const quadratic = (control: ContourPoint, end: { x: number; y: number }) => {
    // Raise to a cubic: the control points lie 2/3 of the way to the control.
    verbs.push(GeometryVerb.cubic);
    coords.push(
      current.x + (2 / 3) * (control.x - current.x),
      current.y + (2 / 3) * (control.y - current.y),
      end.x + (2 / 3) * (control.x - end.x),
      end.y + (2 / 3) * (control.y - end.y),
      end.x,
      end.y,
    );
    current = { ...end, onCurve: true };
  };

  let pending: ContourPoint | undefined;
  const closed = [...ordered.slice(1), ordered[0]];
  for (const point of closed) {
    if (point.onCurve) {
      if (pending) quadratic(pending, point);
      else {
        verbs.push(GeometryVerb.line);
        coords.push(point.x, point.y);
        current = point;
      }
      pending = undefined;
    } else {
      if (pending) {
        quadratic(pending, { x: (pending.x + point.x) / 2, y: (pending.y + point.y) / 2 });
      }
      pending = point;
    }
  }
}
