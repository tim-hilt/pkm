import { Tool } from "../../core/Core";

/**
 * A toolbar glyph: Excalidraw's own icon (`components/icons.tsx`), stroked
 * with the current colour. `box` is the side of its square view box (24 for
 * the Tabler icons, 20 for Excalidraw's modified ones, as Excalidraw sizes them); `markup` sets its own
 * stroke widths, as Excalidraw's do.
 */
export interface Glyph {
  box: number;
  markup: string;
}

/** One Tool button: the name that labels it for screen readers and as its tooltip, and its glyph. */
export interface ToolItem {
  tool: Tool;
  name: string;
  glyph: Glyph;
}

/** The SVG element of `glyph`, `size` CSS pixels square. */
export const glyphSvg = ({ box, markup }: Glyph, size: number): string =>
  `<svg viewBox="0 0 ${box} ${box}" width="${size}" height="${size}" fill="none" stroke="currentColor" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${markup}</svg>`;

/** The label of the Tool lock button, as in the native app. */
export const toolLockLabel = "Keep tool selected";

const tabler = (strokeWidth: number, markup: string): Glyph => ({
  box: 24,
  markup: `<g stroke-width="${strokeWidth}">${markup}</g>`,
});
const modified = (strokeWidth: number, markup: string): Glyph => ({
  box: 20,
  markup: `<g stroke-width="${strokeWidth}">${markup}</g>`,
});

const hand = tabler(
  1.25,
  `<path d="M8 13v-7.5a1.5 1.5 0 0 1 3 0v6.5"/><path d="M11 5.5v-2a1.5 1.5 0 1 1 3 0v8.5"/><path d="M14 5.5a1.5 1.5 0 0 1 3 0v6.5"/><path d="M17 7.5a1.5 1.5 0 0 1 3 0v8.5a6 6 0 0 1-6 6h-2h.208a6 6 0 0 1-5.012-2.7a69.74 69.74 0 0 1-.196-.3c-.312-.479-1.407-2.388-3.286-5.728a1.5 1.5 0 0 1 .536-2.022a1.867 1.867 0 0 1 2.28.28l1.47 1.47"/>`,
);
// Excalidraw draws its Tabler pointer in a 22 x 22 view box, a little larger.
const pointer: Glyph = {
  ...tabler(
    1.25,
    `<path d="M6 6l4.153 11.793a.365.365 0 0 0 .331.207a.366.366 0 0 0 .332-.207l2.184-4.793l4.787-1.994a.355.355 0 0 0 .213-.323a.355.355 0 0 0-.213-.323l-11.787-4.36z"/><path d="M13.5 13.5l4.5 4.5"/>`,
  ),
  box: 22,
};
const rectangle = tabler(1.5, `<rect x="4" y="4" width="16" height="16" rx="2"/>`);
const diamond = tabler(
  1.5,
  `<path d="M10.5 20.4l-6.9-6.9c-.781-.781-.781-2.219 0-3l6.9-6.9c.781-.781 2.219-.781 3 0l6.9 6.9c.781.781.781 2.219 0 3l-6.9 6.9c-.781.781-2.219.781-3 0z"/>`,
);
const ellipse = tabler(1.5, `<circle cx="12" cy="12" r="9"/>`);
const arrow = tabler(1.5, `<path d="M5 12h14M15 16l4-4M15 8l4 4"/>`);
const line = modified(1.5, `<path d="M4.167 10h11.666"/>`);
const freedraw = modified(
  1.25,
  `<path d="m7.643 15.69 7.774-7.773a2.357 2.357 0 1 0-3.334-3.334L4.31 12.357a3.333 3.333 0 0 0-.977 2.357v1.953h1.953c.884 0 1.732-.352 2.357-.977Z"/><path d="m11.25 5.417 3.333 3.333"/>`,
);
const text = tabler(
  1.5,
  `<path d="M4 20h3M14 20h7M6.9 15h6.9M10.2 6.3 16 20"/><polyline points="5 20 11 4 13 4 20 20"/>`,
);
const eraser = tabler(
  1.5,
  `<path d="M19 20h-10.5l-4.21-4.3a1 1 0 0 1 0-1.41l10-10a1 1 0 0 1 1.41 0l5 5a1 1 0 0 1 0 1.41l-9.2 9.3"/><path d="M18 13.3l-6.3-6.3"/>`,
);

/** The button that opens the file picker to insert images. It is no Tool: the images land on the Canvas and the Tool returns to selection. */
export const insertImageItem = {
  name: "Insert image",
  glyph: modified(
    1.25,
    `<path d="M12.5 6.667h.01"/><path d="M4.91 2.625h10.18a2.284 2.284 0 0 1 2.285 2.284v10.182a2.284 2.284 0 0 1-2.284 2.284H4.909a2.284 2.284 0 0 1-2.284-2.284V4.909a2.284 2.284 0 0 1 2.284-2.284Z"/><path d="m3.333 12.5 3.334-3.333c.773-.745 1.726-.745 2.5 0l4.166 4.166"/><path d="m11.667 11.667.833-.834c.774-.744 1.726-.744 2.5 0l1.667 1.667"/>`,
  ),
};

/** Excalidraw's toolbar island, in its order (after the Tool lock). */
export const toolItems: readonly ToolItem[] = [
  { tool: Tool.Hand, name: "Hand", glyph: hand },
  { tool: Tool.Selection, name: "Selection", glyph: pointer },
  { tool: Tool.Rectangle, name: "Rectangle", glyph: rectangle },
  { tool: Tool.Diamond, name: "Diamond", glyph: diamond },
  { tool: Tool.Ellipse, name: "Ellipse", glyph: ellipse },
  { tool: Tool.Arrow, name: "Arrow", glyph: arrow },
  { tool: Tool.Line, name: "Line", glyph: line },
  { tool: Tool.Freedraw, name: "Draw", glyph: freedraw },
  { tool: Tool.Text, name: "Text", glyph: text },
  { tool: Tool.Eraser, name: "Eraser", glyph: eraser },
];

// Excalidraw's padlock body and keyhole; the open shackle is its masked
// `UnlockedIcon` shackle, drawn here as the plain stroke it renders as.
const padlock = `<path d="M13.542 8.542H6.458a2.5 2.5 0 0 0-2.5 2.5v3.75a2.5 2.5 0 0 0 2.5 2.5h7.084a2.5 2.5 0 0 0 2.5-2.5v-3.75a2.5 2.5 0 0 0-2.5-2.5Z"/><path d="M10 13.958a1.042 1.042 0 1 0 0-2.083 1.042 1.042 0 0 0 0 2.083Z"/>`;
const lockedGlyph = modified(
  1.25,
  `${padlock}<path d="M6.667 8.333V5.417C6.667 3.806 8.159 2.5 10 2.5c1.841 0 3.333 1.306 3.333 2.917v2.916"/>`,
);
const unlockedGlyph = modified(
  1.25,
  `${padlock}<path d="M6.399 8.333V5.175c0-.93.401-1.823 1.116-2.48a3.981 3.981 0 0 1 2.693-1.028c1.01 0 1.98.37 2.694 1.027.715.658 1.116 1.55 1.116 2.481"/>`,
);

export const lockGlyph = (locked: boolean): Glyph => (locked ? lockedGlyph : unlockedGlyph);

/** The index of the button that arrow keys, Home and End move focus to, or undefined for other keys. */
export function focusTarget(key: string, current: number, count: number): number | undefined {
  switch (key) {
    case "ArrowRight":
    case "ArrowDown":
      return (current + 1) % count;
    case "ArrowLeft":
    case "ArrowUp":
      return (current - 1 + count) % count;
    case "Home":
      return 0;
    case "End":
      return count - 1;
    default:
      return undefined;
  }
}

/**
 * One button of the phone toolbar. The phone bar has no room for every Tool,
 * so a family of Tools shares a slot: tapping the slot's armed Tool again
 * offers the others of its family.
 */
export interface MobileToolSlot {
  /** The family, the slot's default Tool first. */
  tools: readonly Tool[];
}

/** Excalidraw's phone order; Insert image and Frame have no Tool yet. */
export const mobileToolSlots: readonly MobileToolSlot[] = [
  [Tool.Hand],
  [Tool.Selection],
  [Tool.Freedraw],
  [Tool.Eraser],
  [Tool.Rectangle, Tool.Diamond, Tool.Ellipse],
  [Tool.Arrow, Tool.Line],
  [Tool.Text],
].map((tools) => ({ tools }));

/** What a slot shows: the armed Tool if it is of the family, else the family's last chosen one, else its default. */
export function shownTool(slot: MobileToolSlot, current: Tool, last: Tool | undefined): Tool {
  if (slot.tools.includes(current)) return current;
  if (last !== undefined && slot.tools.includes(last)) return last;
  return slot.tools[0];
}
