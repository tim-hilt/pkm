import { darkModeColor } from "../../core/Core";

/**
 * Excalidraw's colour palette (`COLOR_PALETTE`, open-color) as the colour
 * picker of the Properties panel offers it: a grid of colours, the five
 * shades of a grid colour, and any other colour as a hex code. A port of the
 * Swift `ColorPalette`.
 */

/** Which property a colour is picked for; it decides the grid's shade. */
export type ColorKind = "stroke" | "background";

interface Hue {
  name: string;
  /** Light to dark (open-color 50/200/400/600/800). */
  shades: readonly string[];
}

const hues: readonly Hue[] = [
  { name: "Gray", shades: ["#f8f9fa", "#e9ecef", "#ced4da", "#868e96", "#343a40"] },
  { name: "Red", shades: ["#fff5f5", "#ffc9c9", "#ff8787", "#fa5252", "#e03131"] },
  { name: "Pink", shades: ["#fff0f6", "#fcc2d7", "#f783ac", "#e64980", "#c2255c"] },
  { name: "Grape", shades: ["#f8f0fc", "#eebefa", "#da77f2", "#be4bdb", "#9c36b5"] },
  { name: "Violet", shades: ["#f3f0ff", "#d0bfff", "#9775fa", "#7950f2", "#6741d9"] },
  { name: "Blue", shades: ["#e7f5ff", "#a5d8ff", "#4dabf7", "#228be6", "#1971c2"] },
  { name: "Cyan", shades: ["#e3fafc", "#99e9f2", "#3bc9db", "#15aabf", "#0c8599"] },
  { name: "Teal", shades: ["#e6fcf5", "#96f2d7", "#38d9a9", "#12b886", "#099268"] },
  { name: "Green", shades: ["#ebfbee", "#b2f2bb", "#69db7c", "#40c057", "#2f9e44"] },
  { name: "Yellow", shades: ["#fff9db", "#ffec99", "#ffd43b", "#fab005", "#f08c00"] },
  { name: "Orange", shades: ["#fff4e6", "#ffd8a8", "#ffa94d", "#fd7e14", "#e8590c"] },
  { name: "Bronze", shades: ["#f8f1ee", "#eaddd7", "#d2bab0", "#a18072", "#846358"] },
];

/** The colours of the top-picks rows of the panel. */
export const strokeTopPicks = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
export const backgroundTopPicks = ["transparent", "#ffc9c9", "#b2f2bb", "#a5d8ff", "#ffec99"];

/** The picker grid, row by row (5 × 3), in Excalidraw's order. */
export function grid(kind: ColorKind): string[] {
  // Excalidraw's default shade per kind: dark for strokes, light for backgrounds.
  const shadeIndex = kind === "stroke" ? 4 : 1;
  const shade = (hue: number) => hues[hue].shades[shadeIndex];
  // gray, red, pink, grape, violet, blue, cyan, teal, green, yellow, orange, bronze
  return [
    "transparent",
    "#ffffff",
    shade(0),
    "#1e1e1e",
    shade(11),
    shade(6),
    shade(5),
    shade(4),
    shade(3),
    shade(2),
    shade(8),
    shade(7),
    shade(9),
    shade(10),
    shade(1),
  ];
}

/** The five shades of a palette colour, light to dark; `undefined` for any other colour (black, white, transparent, a custom hex code). */
export function shadesOf(color: string | undefined): readonly string[] | undefined {
  const lower = color?.toLowerCase();
  return hues.find((hue) => lower !== undefined && hue.shades.includes(lower))?.shades;
}

/** The colour a swatch is drawn in: the colour itself, or in Dark mode the core's display colour, as on the dark Canvas. Only the drawing is affected; the chosen value and its spoken name stay `color`. */
export function displayColor(color: string, dark: boolean): string {
  return dark ? darkModeColor(color) : color;
}

/** The colour a typed hex code stands for, `#` optional; `undefined` if it is not 3, 4, 6 or 8 hex digits (Excalidraw's rule). */
export function colorFromHex(input: string): string | undefined {
  const digits = input.trim().toLowerCase().replace(/^#/, "");
  return [3, 4, 6, 8].includes(digits.length) && /^[0-9a-f]+$/.test(digits)
    ? `#${digits}`
    : undefined;
}

const topPickNames: Record<string, string> = {
  transparent: "Transparent",
  "#1e1e1e": "Black",
  "#ffffff": "White",
  "#e03131": "Red",
  "#2f9e44": "Green",
  "#1971c2": "Blue",
  "#f08c00": "Orange",
  "#ffc9c9": "Light red",
  "#b2f2bb": "Light green",
  "#a5d8ff": "Light blue",
  "#ffec99": "Light yellow",
};

/** What a screen reader reads for a colour: Excalidraw's names for the top picks, the hue and shade for other palette colours. */
export function colorName(color: string): string {
  const lower = color.toLowerCase();
  if (lower in topPickNames) return topPickNames[lower];
  for (const hue of hues) {
    const shade = hue.shades.indexOf(lower);
    if (shade >= 0) return `${hue.name}, shade ${shade + 1}`;
  }
  return `Colour ${lower}`;
}

/** Whether `color` draws nothing: `transparent`, anything that is no hex code, or a zero alpha. */
export function isTransparent(color: string): boolean {
  if (!color.startsWith("#")) return true;
  const digits = color.slice(1);
  if (digits.length === 4) return digits.endsWith("0");
  if (digits.length === 8) return digits.endsWith("00");
  return false;
}
