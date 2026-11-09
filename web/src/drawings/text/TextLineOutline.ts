import { type GeometryTextLine, GeometryTextAlign } from "../rendering/GeometryBuffers";
import { Excalifont } from "./Excalifont";

/** A path in the format of the core's geometry. */
export interface OutlinePath {
  verbs: number[];
  coords: number[];
}

/**
 * A line of text as one filled path in Scene coordinates: the outlines of its
 * glyphs, scaled, aligned and turned as the core's `TextLine` says. Drawn with
 * the nonzero rule like any glyph outline. A port of the Swift `TextLineOutline`.
 *
 * @param text The characters of `line`.
 */
export function textLineOutline(
  line: GeometryTextLine,
  text: string,
  font: Excalifont,
): OutlinePath {
  const layout = font.layout(text);
  const scale = line.fontSize / Excalifont.designSize;
  const shift =
    line.align === GeometryTextAlign.center
      ? -layout.width / 2
      : line.align === GeometryTextAlign.right
        ? -layout.width
        : 0;
  const sin = Math.fround(Math.sin(line.angle));
  const cos = Math.fround(Math.cos(line.angle));

  const path: OutlinePath = { verbs: [], coords: [] };
  for (const glyph of layout.glyphs) {
    path.verbs.push(...glyph.outline.verbs);
    const { coords } = glyph.outline;
    for (let i = 0; i < coords.length; i += 2) {
      const local = (glyph.x + shift + coords[i]) * scale;
      const down = coords[i + 1] * scale;
      path.coords.push(line.x + local * cos - down * sin, line.y + local * sin + down * cos);
    }
  }
  return path;
}
