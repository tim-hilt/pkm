import { Cursor } from "../core/Core";

// The web's look of the core's `Cursor` (ADR-0006: the core decides which
// cursor shows, the client only how it looks); a port of CanvasCursor.swift.
// The browser has CSS cursors for most. The eraser is the same small disc as
// on Swift, 10 points wide, drawn with a white fill and a black outline.

const svgCursor = (svg: string, size: number, fallback: string) =>
  `url("data:image/svg+xml,${encodeURIComponent(svg)}") ${size / 2} ${size / 2}, ${fallback}`;

const eraser = svgCursor(
  `<svg xmlns="http://www.w3.org/2000/svg" width="14" height="14"><circle cx="7" cy="7" r="5" fill="white" stroke="black" stroke-width="1"/></svg>`,
  14,
  "crosshair",
);

/** The CSS `cursor` value for the core's cursor. */
export function cssCursor(cursor: Cursor): string {
  switch (cursor) {
    case Cursor.Arrow:
      return "default";
    case Cursor.Crosshair:
      return "crosshair";
    case Cursor.Text:
      return "text";
    case Cursor.Move:
      return "move";
    case Cursor.Grab:
      return "grab";
    case Cursor.Grabbing:
      return "grabbing";
    case Cursor.Pointer:
      return "pointer";
    case Cursor.Eraser:
      return eraser;
    case Cursor.ResizeNorthSouth:
      return "ns-resize";
    case Cursor.ResizeEastWest:
      return "ew-resize";
    case Cursor.ResizeNorthWestSouthEast:
      return "nwse-resize";
    case Cursor.ResizeNorthEastSouthWest:
      return "nesw-resize";
  }
}
