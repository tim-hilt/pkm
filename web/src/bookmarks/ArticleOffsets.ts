/**
 * Offsets in an Article's rendered text. The Reader's DOM holds exactly the Article's Markdown (the
 * markers hidden, not left out), so a position in the DOM and a UTF-16 offset into the stored
 * Markdown are one another: Markup and Reading Progress are kept by the latter (Bookmarks ADR-0002).
 */

/** The offset of `offset` in `node`, a position in `root`. */
export function offsetOf(root: Node, node: Node, offset: number): number {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.setEnd(node, offset);
  return range.toString().length;
}

/** The range of the text of `root` from `start` up to `end`; clamped to its end. */
export function rangeOf(root: Node, start: number, end: number): Range {
  const range = document.createRange();
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let position = 0;
  let from = false;
  let last: Text | undefined;
  for (let node = walker.nextNode() as Text | null; node; node = walker.nextNode() as Text | null) {
    last = node;
    const stop = position + node.data.length;
    if (!from && start < stop) {
      range.setStart(node, start - position);
      from = true;
    }
    if (from && end <= stop) {
      range.setEnd(node, end - position);
      return range;
    }
    position = stop;
  }
  if (last) {
    if (!from) range.setStart(last, last.data.length);
    range.setEnd(last, last.data.length);
  }
  return range;
}

/**
 * Where the first character shown at or after `offset` is, in the viewport; `undefined` if none is
 * (the text from there on is hidden). Characters of hidden markers have no box, so it looks on.
 */
export function rectAt(root: Node, offset: number): DOMRect | undefined {
  const reach = 64;
  for (let at = offset; at < offset + reach; at++) {
    const rect = rangeOf(root, at, at + 1).getClientRects()[0];
    if (rect && rect.height > 0) return rect;
  }
  return undefined;
}

/** The offset of the character at the point (`x`, `y`) of the viewport, if that is in `root`'s text. */
export function offsetAtPoint(root: Node, x: number, y: number): number | undefined {
  const position = document.caretPositionFromPoint?.(x, y);
  const at = position ? { node: position.offsetNode, offset: position.offset } : caretRange(x, y);
  return at && root.contains(at.node) ? offsetOf(root, at.node, at.offset) : undefined;
}

/** Safari's and Chrome's older spelling of `caretPositionFromPoint`, which they lack or lacked. */
function caretRange(x: number, y: number): { node: Node; offset: number } | undefined {
  const range = (
    document as Document & { caretRangeFromPoint?: (x: number, y: number) => Range | null }
  ).caretRangeFromPoint?.(x, y);
  return range ? { node: range.startContainer, offset: range.startOffset } : undefined;
}
