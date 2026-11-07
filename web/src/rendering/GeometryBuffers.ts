// TS view of the core's geometry output (ADR-0007), a port of the Swift
// client's `GeometryBuffers.swift`. Parts and text lines so far; images follow
// with the ticket that draws them. The format is documented in the core's
// `geometry` module; this file mirrors it.

/** Mirror of the core's `Part` record, read straight out of wasm memory. */
export interface GeometryPart {
  /** Core-assigned Element handle; with `version` the cache key. */
  element: number;
  version: number;
  kind: number;
  /** `0xRRGGBBAA`. */
  color: number;
  /** Element opacity, 0...1. */
  opacity: number;
  /** Line width in Scene units; 0 for fills. */
  width: number;
  /** Range of this part's verbs in `GeometryBuffers.verbs`. */
  verbStart: number;
  verbCount: number;
  /** Index of this part's first operand in `GeometryBuffers.coords`. */
  coordStart: number;
  /** Length of the drawn stretches and of the gaps of a dashed line; both 0 for a solid line. */
  dashOn: number;
  dashOff: number;
  /** A `GeometryFillRule` number; 0 for strokes. */
  fillRule: number;
  /**
   * Upright rectangle in Scene coordinates (top left, size) where nothing of
   * the part is drawn. A zero width or height is no hole.
   */
  holeX: number;
  holeY: number;
  holeWidth: number;
  holeHeight: number;
}

/**
 * Mirror of the core's `TextLine` record: one line of text to draw, set in
 * Excalifont, read straight out of wasm memory.
 */
export interface GeometryTextLine {
  /** Core-assigned Element handle; with `version` the cache key. */
  element: number;
  version: number;
  /**
   * Number of parts before this line in z-order: draw the line after the
   * parts before this index and before the part at it.
   */
  partIndex: number;
  /** `0xRRGGBBAA`. */
  color: number;
  /** Element opacity, 0...1. */
  opacity: number;
  fontSize: number;
  /** The Element's font family. Every family is drawn in Excalifont. */
  fontFamily: number;
  /** A `GeometryTextAlign` number. */
  align: number;
  /** Where the line is anchored on its baseline, in Scene units, already turned by `angle`. */
  x: number;
  y: number;
  /** Clockwise rotation in radians about `(x, y)`. */
  angle: number;
  /** Range of the line's UTF-8 bytes in `GeometryBuffers.text`. */
  textStart: number;
  textLength: number;
}

/** Where a line of text sits relative to its anchor, as in the core's `TextAlign`. */
export const GeometryTextAlign = { left: 0, center: 1, right: 2 } as const;

/** Constant-width outline with round caps and joins. */
export const strokeKind = 0;
/** The area enclosed by the path, whose subpaths are implicitly closed. */
export const fillKind = 1;

/** The hole as min x, min y, max x, max y; empty (nothing is inside) if there is none. */
export function holeOf(part: GeometryPart): [number, number, number, number] {
  return part.holeWidth > 0 && part.holeHeight > 0
    ? [part.holeX, part.holeY, part.holeX + part.holeWidth, part.holeY + part.holeHeight]
    : [0, 0, 0, 0];
}

/** The dash pattern, if the part is dashed. It starts over at every `move`. */
export function dashOf(part: GeometryPart): { on: number; off: number } | undefined {
  return part.dashOn > 0 && part.dashOff > 0 ? { on: part.dashOn, off: part.dashOff } : undefined;
}

/** A part with the given fields and the rest zero. */
export function makePart(fields: Partial<GeometryPart>): GeometryPart {
  return {
    element: 0,
    version: 1,
    kind: strokeKind,
    color: 0x1e1e1eff,
    opacity: 1,
    width: 2,
    verbStart: 0,
    verbCount: 0,
    coordStart: 0,
    dashOn: 0,
    dashOff: 0,
    fillRule: 0,
    holeX: 0,
    holeY: 0,
    holeWidth: 0,
    holeHeight: 0,
    ...fields,
  };
}

/** Path operation numbers, as in the core's `Verb`. */
export const GeometryVerb = {
  /** Operands: x y. */
  move: 0,
  /** Operands: x y. */
  line: 1,
  /** Operands: x1 y1 x2 y2 x y (two control points, then the end point). */
  cubic: 2,
} as const;

/** Which points a fill covers where its path crosses itself, as in the core's `FillRule`. */
export const GeometryFillRule = { nonZero: 0, evenOdd: 1 } as const;

/**
 * Records of one kind: an array of them, or a window onto wasm memory that
 * decodes a record only when it is asked for.
 */
export interface Records<T> {
  readonly length: number;
  at(index: number): T;
}

export function recordsOf<T>(array: readonly T[]): Records<T> {
  return { length: array.length, at: (index) => array[index] };
}

/**
 * The core's geometry buffers, borrowed. Only valid inside the
 * `GeometrySource.withBuffers` call that produced them: the views belong to
 * wasm memory, which may move when it grows.
 */
export interface GeometryBuffers {
  parts: Records<GeometryPart>;
  verbs: Uint8Array;
  coords: Float32Array;
  /** Lines of text, interleaved with `parts` in z-order by `partIndex`. */
  textLines: Records<GeometryTextLine>;
  /** The characters of all lines, as UTF-8. */
  text: Uint8Array;
}

const utf8 = new TextDecoder();

/** The characters of `line`. */
export function stringOf(buffers: GeometryBuffers, line: GeometryTextLine): string {
  return utf8.decode(buffers.text.subarray(line.textStart, line.textStart + line.textLength));
}

/** The verbs of `part`. */
export function verbsOf(buffers: GeometryBuffers, part: GeometryPart): Uint8Array {
  return buffers.verbs.subarray(part.verbStart, part.verbStart + part.verbCount);
}

/**
 * The operands from the start of `part` to the end of the buffer; the part's
 * verbs say how many it consumes.
 */
export function coordsOf(buffers: GeometryBuffers, part: GeometryPart): Float32Array {
  return buffers.coords.subarray(part.coordStart);
}

/**
 * Geometry the renderer can draw: the core's `Geometry`, or hand-built
 * geometry in tests.
 */
export interface GeometrySource {
  /**
   * Tells whether cached per-Element results are still valid, see the core's
   * `Geometry::generation`.
   */
  generation(): number;
  /** Runs `body` with the buffers, which must not escape it. */
  withBuffers<R>(body: (buffers: GeometryBuffers) => R): R;
}

/** Sizes in bytes of the core's records; `readGeometry` checks them against the core's. */
export const partStride = 64;
export const textLineStride = 52;

/** What `readGeometry` needs of the wasm `Geometry` object. */
export interface WasmGeometry {
  partCount(): number;
  partStride(): number;
  partsAddress(): number;
  verbCount(): number;
  verbsAddress(): number;
  coordCount(): number;
  coordsAddress(): number;
  textLineCount(): number;
  textLineStride(): number;
  textLinesAddress(): number;
  textByteCount(): number;
  textAddress(): number;
}

const partFields = partStride / 4;
const textLineFields = textLineStride / 4;

/**
 * Views onto wasm memory. A view is only good until memory grows, which
 * replaces its `ArrayBuffer`; `of` makes new ones whenever that happened and
 * hands out the same ones while it did not.
 */
export class MemoryViews {
  private buffer: ArrayBufferLike | undefined;
  private u32 = new Uint32Array(0);
  private f32 = new Float32Array(0);
  private u8 = new Uint8Array(0);

  constructor(private readonly memory: WebAssembly.Memory) {}

  get words(): Uint32Array {
    return this.refresh().u32;
  }
  get floats(): Float32Array {
    return this.refresh().f32;
  }
  get bytes(): Uint8Array {
    return this.refresh().u8;
  }

  private refresh(): this {
    if (this.buffer !== this.memory.buffer) {
      this.buffer = this.memory.buffer;
      this.u32 = new Uint32Array(this.buffer);
      this.f32 = new Float32Array(this.buffer);
      this.u8 = new Uint8Array(this.buffer);
    }
    return this;
  }
}

/**
 * Runs `body` with the buffers of `geometry`, read in place without copying.
 * The buffers must not escape `body`.
 */
export function readGeometry<R>(
  geometry: WasmGeometry,
  memory: MemoryViews,
  body: (buffers: GeometryBuffers) => R,
): R {
  if (geometry.partStride() !== partStride) {
    throw new Error("GeometryPart no longer mirrors the core's Part record");
  }
  if (geometry.textLineStride() !== textLineStride) {
    throw new Error("GeometryTextLine no longer mirrors the core's TextLine record");
  }
  const { words, floats, bytes } = memory;
  const partsStart = geometry.partsAddress() / 4;
  const verbsStart = geometry.verbsAddress();
  const coordsStart = geometry.coordsAddress() / 4;
  const parts: Records<GeometryPart> = {
    length: geometry.partCount(),
    at(index) {
      const o = partsStart + index * partFields;
      return {
        element: words[o],
        version: words[o + 1],
        kind: words[o + 2],
        color: words[o + 3],
        opacity: floats[o + 4],
        width: floats[o + 5],
        verbStart: words[o + 6],
        verbCount: words[o + 7],
        coordStart: words[o + 8],
        dashOn: floats[o + 9],
        dashOff: floats[o + 10],
        fillRule: words[o + 11],
        holeX: floats[o + 12],
        holeY: floats[o + 13],
        holeWidth: floats[o + 14],
        holeHeight: floats[o + 15],
      };
    },
  };
  const linesStart = geometry.textLinesAddress() / 4;
  const textLines: Records<GeometryTextLine> = {
    length: geometry.textLineCount(),
    at(index) {
      const o = linesStart + index * textLineFields;
      return {
        element: words[o],
        version: words[o + 1],
        partIndex: words[o + 2],
        color: words[o + 3],
        opacity: floats[o + 4],
        fontSize: floats[o + 5],
        fontFamily: words[o + 6],
        align: words[o + 7],
        x: floats[o + 8],
        y: floats[o + 9],
        angle: floats[o + 10],
        textStart: words[o + 11],
        textLength: words[o + 12],
      };
    },
  };
  const textStart = geometry.textAddress();
  return body({
    parts,
    verbs: bytes.subarray(verbsStart, verbsStart + geometry.verbCount()),
    coords: floats.subarray(coordsStart, coordsStart + geometry.coordCount()),
    textLines,
    text: bytes.subarray(textStart, textStart + geometry.textByteCount()),
  });
}

/**
 * What one Element contributes to the geometry. An Element is drawn by a run
 * of parts or by a run of lines of text, never a mix.
 */
export interface ElementGeometry {
  element: number;
  version: number;
  /** Indices from `start` up to but excluding `end`, into `GeometryBuffers.parts` or `.textLines`. */
  content: "parts" | "textLines";
  start: number;
  end: number;
}

/** Every Element's share of the buffers, in z-order (bottom first). */
export function* elementsInZOrder(buffers: GeometryBuffers): Generator<ElementGeometry> {
  const { parts, textLines } = buffers;
  let part = 0;
  let line = 0;
  while (true) {
    // Lines come before the part their `partIndex` names.
    if (line < textLines.length && textLines.at(line).partIndex <= part) {
      const first = textLines.at(line);
      let end = line + 1;
      while (end < textLines.length && textLines.at(end).element === first.element) end++;
      yield {
        element: first.element,
        version: first.version,
        content: "textLines",
        start: line,
        end,
      };
      line = end;
    } else if (part < parts.length) {
      const first = parts.at(part);
      let end = part + 1;
      while (end < parts.length && parts.at(end).element === first.element) end++;
      yield { element: first.element, version: first.version, content: "parts", start: part, end };
      part = end;
    } else {
      return;
    }
  }
}
