import { Inline, parseTitle } from "../core/Core";
import { make } from "../Dom";

/** A formatted span of a Title, plain copy of the core's: `start..end` in UTF-16 units, its first and last `markerLen` being markers. */
export interface TitleSpan {
  kind: Inline;
  start: number;
  end: number;
  markerLen: number;
}

/** The Inline formatting of `title`, outer spans first. */
export function titleSpans(title: string): TitleSpan[] {
  return parseTitle(title).map((span) => {
    const plain = {
      kind: span.kind,
      start: span.start,
      end: span.end,
      markerLen: span.markerLen,
    };
    span.free();
    return plain;
  });
}

/** What to draw over `start..end` of a text, nested in the ranges it lies in. */
export interface Range {
  start: number;
  end: number;
  element: HTMLElement;
}

/** The Inline formatting spans of a Title as elements, with their markers (class `md-marker`) as ranges of their own. */
export function formatRanges(title: string): Range[] {
  return titleSpans(title).flatMap(({ kind, start, end, markerLen }) => {
    const tag = kind === Inline.Code ? "code" : kind === Inline.Strong ? "strong" : "em";
    const element = make(tag, kind === Inline.Code ? "md-code" : "");
    const marker = () => make("span", "md-marker");
    return [
      { start, end, element },
      { start, end: start + markerLen, element: marker() },
      { start: end - markerLen, end, element: marker() },
    ];
  });
}

/**
 * `text` as nodes with `ranges` drawn over it. Ranges nest; one that would cross
 * the end of the range it starts in is left out. Equal starts put the longer range outside.
 */
export function drawRanges(text: string, ranges: Range[]): (Node | string)[] {
  const root: (Node | string)[] = [];
  const open: Range[] = [];
  let at = 0;
  const put = (node: Node | string) => {
    const parent = open.at(-1);
    if (parent) parent.element.append(node);
    else root.push(node);
  };
  const flush = (to: number) => {
    if (to > at) put(text.slice(at, to));
    at = Math.max(at, to);
  };
  const sorted = [...ranges].sort((a, b) => a.start - b.start || b.end - a.end);
  for (const range of sorted) {
    while (open.length > 0 && open[open.length - 1].end <= range.start) {
      flush(open[open.length - 1].end);
      open.pop();
    }
    if (open.length > 0 && range.end > open[open.length - 1].end) continue;
    flush(range.start);
    put(range.element);
    open.push(range);
  }
  while (open.length > 0) {
    flush(open[open.length - 1].end);
    open.pop();
  }
  flush(text.length);
  return root;
}

/** A Title as the rows and the Parent line show it: formatted; the CSS hides its markers (`.md-marker`). */
export function titleNodes(title: string): (Node | string)[] {
  return drawRanges(title, formatRanges(title));
}
