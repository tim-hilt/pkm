import { Block, parseDescription } from "../core/Core";
import { make } from "../Dom";
import type { DescriptionImages } from "./DescriptionImages";
import { type Range, type TitleSpan, drawRanges, inlineRanges } from "./TitleText";

/** The UTF-16 range of a Description that shows its Markdown raw. */
export interface Raw {
  start: number;
  end: number;
}

/**
 * The Description `text` as nodes in Live Preview: every line rendered (its
 * markers class `md-marker`) but the `raw` range, whose markers also get
 * `md-shown`. The nodes hold exactly `text`, so a caret offset in them is one in
 * the text. Rendered links are `a` elements with their target in `data-href`.
 * An Image line holds its reference as text, hidden unless raw, and an `img`
 * (no text) that `images` fills, with the reference in `data-reference`.
 */
export function descriptionNodes(
  text: string,
  raw?: Raw,
  images?: DescriptionImages,
): (Node | string)[] {
  const isRaw = (start: number, end: number) =>
    raw !== undefined && raw.start <= start && end <= raw.end;
  const parsed = parseDescription(text);
  const ranges: Range[] = [];
  const marker = (start: number, end: number, shown: boolean, className = "") => {
    if (end <= start) return undefined;
    const element = make("span", `md-marker ${shown ? "md-shown" : className}`.trim());
    ranges.push({ start, end, element });
    return element;
  };
  const blocks = parsed.blocks;
  const pictures: [HTMLElement, string][] = [];
  for (const block of blocks) {
    const { start, end, markerStart, markerEnd, level, number } = block;
    const shown = isRaw(start, end);
    switch (block.kind) {
      case Block.Heading: {
        const heading = make("span", `md-heading md-h${level}`);
        heading.setAttribute("role", "heading");
        heading.setAttribute("aria-level", String(level));
        ranges.push({ start, end, element: heading });
        marker(markerStart, markerEnd, shown);
        break;
      }
      case Block.Quote:
        ranges.push({ start, end, element: make("span", "md-quote") });
        marker(markerStart, markerEnd, shown);
        break;
      case Block.CodeFence:
        // Each fence line's marker holds its line break, so hiding it hides the line.
        ranges.push({
          start,
          end: Math.max(end, block.closingEnd),
          element: make("span", "md-fence"),
        });
        marker(markerStart, markerEnd, shown);
        marker(block.closingStart, block.closingEnd, shown);
        break;
      case Block.Image: {
        const element = make("span", `md-image${shown ? " md-shown" : ""}`);
        ranges.push({ start, end, element });
        pictures.push([element, block.target]);
        break;
      }
      case Block.Bullet:
      case Block.Numbered: {
        // Rendered, a bullet or the number reached is drawn in the marker's place by the CSS.
        const item = block.kind === Block.Bullet ? "md-bullet" : "md-number";
        const element = marker(markerStart, markerEnd, shown, item);
        if (element) element.dataset.number = String(number);
        break;
      }
    }
  }
  const inlineSpans = parsed.inlines;
  const inlines: TitleSpan[] = inlineSpans.map(({ kind, start, end, markerLen }) => ({
    kind,
    start,
    end,
    markerLen,
  }));
  ranges.push(...inlineRanges(inlines, isRaw));
  const links = parsed.links;
  for (const link of links) {
    const shown = isRaw(link.start, link.end);
    const anchor = make("a", "md-link");
    if (!shown) anchor.dataset.href = link.target;
    ranges.push({ start: link.textStart, end: link.textEnd, element: anchor });
    marker(link.start, link.textStart, shown);
    marker(link.textEnd, link.end, shown);
  }
  const nodes = drawRanges(text, ranges);
  for (const [element, target] of pictures) {
    const image = make("img", "md-picture");
    image.alt = "Image";
    image.dataset.reference = target;
    images?.show(image, target);
    element.append(image);
  }
  for (const span of [...blocks, ...inlineSpans, ...links, parsed]) span.free();
  return nodes;
}
