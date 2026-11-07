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
