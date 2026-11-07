/**
 * Which of excalidraw.com's layouts a Drawing's chrome uses, decided from the
 * size of its container alone, so that it follows window resizing. Excalidraw's
 * in-between "tablet" form factor gets the desktop layout: the spec has only
 * two. A port of the Swift `DrawingLayout`.
 */
export type Layout = "desktop" | "mobile";

export function layoutFor(width: number, height: number): Layout {
  const phone = width <= 599 || (height < 500 && width < 1000);
  return phone ? "mobile" : "desktop";
}

/** Margin of every floating control from the container's edge. */
export const margin = 16;
/** Top of the desktop Properties panel, below the toolbar. */
export const propertiesTopBelowToolbar = 76;
/** The desktop Properties panel's width. */
export const propertiesWidth = 195;
/** Clear room kept between the panel and a toolbar that shares its top. */
export const propertiesGap = 80;

/** Top of the desktop Properties panel: level with the toolbar when the centred toolbar leaves the panel (and a gap) clear on the left, otherwise below the toolbar. */
export function propertiesTop(containerWidth: number, toolbarWidth: number): number {
  const toolbarLeft = (containerWidth - toolbarWidth) / 2;
  return toolbarLeft >= margin + propertiesWidth + propertiesGap
    ? margin
    : propertiesTopBelowToolbar;
}

/** Excalidraw's `containerHeight − 166`: the desktop Properties panel scrolls beyond this, clear of the zoom controls. A panel sitting higher than the default top (at `top`) gets that much more room. */
export function propertiesMaxHeight(
  containerHeight: number,
  top = propertiesTopBelowToolbar,
): number {
  return Math.max(0, containerHeight - 166 + (propertiesTopBelowToolbar - top));
}

/** The phone layout as a CSS media query, the same rule as `layoutFor` (the chrome of the other layout is hidden by it, so that the layout follows resizing and rotation without script). */
export const phoneQuery = "(max-width: 599px), (max-height: 499px) and (max-width: 999px)";

/** The phone bottom bar (Action row over the toolbar): Excalidraw's `min(100% − 28, 450)`, centred. */
export function bottomBarWidth(containerWidth: number): number {
  return Math.max(0, Math.min(containerWidth - 28, 450));
}

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

/** Where a phone popover sits. */
export interface PopoverPlacement {
  /** The popover's frame in the container: its content size, capped to the container's width less the edge margins and to `maxHeight`. */
  frame: Rect;
  /** The pointer tip's x in the container: always on the anchor's centre. */
  pointerX: number;
  /** The tallest the popover may be; it scrolls beyond. */
  maxHeight: number;
}

/** Least distance of a popover from the container's edges. */
export const popoverEdgeMargin = 8;
/** Gap between a popover's bottom and the bar it belongs to. */
export const popoverGap = 12;

/** A popover as wide as its content, centred on the anchor but kept `popoverEdgeMargin` from the container's edges, its bottom `popoverGap` above the bar. `barTop` is the bar's top in the container. */
export function popoverPlacement(
  anchor: Rect,
  content: { width: number; height: number },
  container: { width: number },
  barTop: number,
): PopoverPlacement {
  const maxHeight = Math.max(0, barTop - popoverGap - popoverEdgeMargin);
  const height = Math.min(content.height, maxHeight);
  const widest = Math.max(0, container.width - 2 * popoverEdgeMargin);
  const width = Math.min(content.width, widest);
  const midX = anchor.x + anchor.width / 2;
  const x = Math.min(
    Math.max(midX - width / 2, popoverEdgeMargin),
    popoverEdgeMargin + widest - width,
  );
  return {
    frame: { x, y: barTop - popoverGap - height, width, height },
    pointerX: midX,
    maxHeight,
  };
}
