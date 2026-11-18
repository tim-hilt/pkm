/*
 * What the text fields share: the editable element holds nothing but one text
 * (lines joined by "\n", which `white-space: pre-wrap` shows), so that
 * offsets into its `textContent` are the UTF-16 offsets the core counts.
 */

/**
 * The UTF-16 range of `old` that `now` replaced, and what replaced it: all but
 * their common start and end, never splitting a surrogate pair.
 */
export function textEdit(
  old: string,
  now: string,
): { start: number; end: number; inserted: string } {
  const limit = Math.min(old.length, now.length);
  let head = 0;
  while (head < limit && old[head] === now[head]) head++;
  if (head > 0 && (isLowSurrogate(old, head) || isLowSurrogate(now, head))) head--;
  let tail = 0;
  while (tail < limit - head && old[old.length - 1 - tail] === now[now.length - 1 - tail]) tail++;
  if (tail > 0 && isLowSurrogate(old, old.length - tail)) tail--;
  return { start: head, end: old.length - tail, inserted: now.slice(head, now.length - tail) };
}

function isLowSurrogate(text: string, at: number): boolean {
  const unit = text.charCodeAt(at);
  return unit >= 0xdc00 && unit <= 0xdfff;
}

/** The UTF-16 offsets of the selection in `root`'s text, `start` up to `end`; none if it is elsewhere. */
export function selectionOffsets(root: HTMLElement): { start: number; end: number } | undefined {
  const selection = getSelection();
  if (!selection?.anchorNode || !root.contains(selection.anchorNode) || !selection.focusNode)
    return undefined;
  if (!root.contains(selection.focusNode)) return undefined;
  const offset = (node: Node, at: number) => {
    const range = document.createRange();
    range.selectNodeContents(root);
    range.setEnd(node, at);
    return range.toString().length;
  };
  const anchor = offset(selection.anchorNode, selection.anchorOffset);
  const focus = offset(selection.focusNode, selection.focusOffset);
  return { start: Math.min(anchor, focus), end: Math.max(anchor, focus) };
}

/** The UTF-16 offset of the caret in `root`'s text; none if it is elsewhere or a range is selected. */
export function caretOffset(root: HTMLElement): number | undefined {
  const selected = selectionOffsets(root);
  return selected && selected.start === selected.end ? selected.start : undefined;
}

/** Puts the caret at the UTF-16 `offset` of `root`'s text, at the end of a text node rather than the start of the next. */
export function placeCaret(root: HTMLElement, offset: number): void {
  const range = document.createRange();
  range.selectNodeContents(root);
  range.collapse(false);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  let seen = 0;
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const length = node.textContent?.length ?? 0;
    if (seen + length >= offset) {
      range.setStart(node, offset - seen);
      range.collapse(true);
      break;
    }
    seen += length;
  }
  getSelection()?.removeAllRanges();
  getSelection()?.addRange(range);
}
