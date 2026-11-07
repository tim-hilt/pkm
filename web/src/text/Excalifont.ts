import { type GlyphOutline, TrueTypeFont } from "./TrueTypeFont";

export type { GlyphOutline };

/** Measures text for the core; see the core's `TextMeasurer`. */
export interface TextMeasurer {
  /** The advance width in Scene units of `text`, a single line, set at `fontSize`. */
  lineWidth(text: string, fontSize: number): number;
}

/** A line of text set in Excalifont at the design size: each glyph's outline and position, and the line's advance width. */
export interface LineLayout {
  glyphs: {
    outline: GlyphOutline;
    /** Where the glyph's origin is on the baseline. */ x: number;
  }[];
  width: number;
}

/**
 * Excalifont, the only bundled font, a port of the Swift `Excalifont`. It
 * measures text for the core and hands the renderer glyph outlines, cached
 * per glyph, so that text is drawn as filled paths that stay sharp at any
 * zoom (ADR-0008).
 *
 * Every font family of a Drawing is drawn in Excalifont; a character the font
 * lacks shows as its missing-glyph box (proper fallback fonts come later).
 */
export class Excalifont implements TextMeasurer {
  /**
   * The design size: units per em, and the size every line is set at so that
   * advances and outlines are in design units.
   */
  static readonly designSize = 1000;

  private readonly font: TrueTypeFont;
  /** Design units per font unit. */
  private readonly scale: number;
  private readonly glyphs = new Map<number, GlyphOutline>();
  /** The glyph drawn for a character the font lacks. */
  private readonly missingGlyph: number;

  /** Throws if `bytes` is not a font this app can read. */
  constructor(bytes: Uint8Array) {
    this.font = new TrueTypeFont(bytes);
    this.scale = Excalifont.designSize / this.font.unitsPerEm;
    // Excalifont's .notdef (glyph 0) is blank; its box is the glyph after it.
    this.missingGlyph = this.font.outline(0).verbs.length > 0 ? 0 : 1;
  }

  /** Number of distinct glyph outlines made so far; a glyph is made once, however often and wherever it is drawn. */
  get cachedGlyphCount(): number {
    return this.glyphs.size;
  }

  lineWidth(text: string, fontSize: number): number {
    return (this.advances(text).width * fontSize) / Excalifont.designSize;
  }

  /** `text`, a single line, laid out at the design size. */
  layout(text: string): LineLayout {
    const { positions, width } = this.advances(text);
    const glyphs = positions.flatMap(({ glyph, x }) => {
      const outline = this.outline(glyph);
      return outline.verbs.length > 0 ? [{ outline, x }] : [];
    });
    return { glyphs, width };
  }

  /** The glyph of every character of `text` with its pen position, and the total advance, in design units. */
  private advances(text: string): { positions: { glyph: number; x: number }[]; width: number } {
    const positions: { glyph: number; x: number }[] = [];
    let x = 0;
    let previous: number | undefined;
    for (const character of text) {
      const found = this.font.glyphIndex(character.codePointAt(0)!);
      const glyph = found === 0 ? this.missingGlyph : found;
      if (previous !== undefined) x += this.font.kerning(previous, glyph) * this.scale;
      positions.push({ glyph, x });
      x += this.font.advance(glyph) * this.scale;
      previous = glyph;
    }
    return { positions, width: x };
  }

  private outline(glyph: number): GlyphOutline {
    let outline = this.glyphs.get(glyph);
    if (!outline) {
      const raw = this.font.outline(glyph);
      outline = { verbs: raw.verbs, coords: raw.coords.map((value) => value * this.scale) };
      this.glyphs.set(glyph, outline);
    }
    return outline;
  }
}
