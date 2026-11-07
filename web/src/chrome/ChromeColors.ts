/** One colour of the chrome: Excalidraw's light value and, from its dark theme (`theme.scss`), the one for Dark mode. */
interface ChromeColor {
  /** The CSS custom property that holds it. */
  property: string;
  light: string;
  dark: string;
}

/**
 * The chrome's hand-picked colours, ported from the Swift `Chrome`. The
 * accent is Excalidraw's primary purple (`#6965db` light / `#a8a5ff` dark);
 * the selected option and checked colours are derived from it.
 */
const colors: readonly ChromeColor[] = [
  // Ink of the chrome's controls and text.
  { property: "--control-ink", light: "#1b1b1f", dark: "#e3e3e8" },
  { property: "--accent", light: "#6965db", dark: "#a8a5ff" },
  // Text on the accent.
  { property: "--accent-ink", light: "#ffffff", dark: "#1b1b1f" },
  // Island and sheet surface: `--island-bg-color`.
  { property: "--island-fill", light: "#ffffff", dark: "#232329" },
  // Grey control group fill: `--color-surface-low`.
  { property: "--control-fill", light: "#ececf4", dark: "#232329" },
  // Checked tool fill and glyph: `--color-surface-primary-container`, `--color-on-primary-container`.
  { property: "--checked-fill", light: "#e0dfff", dark: "#403e6a" },
  { property: "--checked-ink", light: "#030064", dark: "#e0dfff" },
  // Divider between tool groups.
  { property: "--divider", light: "#f1f0ff", dark: "#46464f" },
  // Colour picker panel: `--color-surface-mid`.
  { property: "--picker-panel", light: "#f7f7f7", dark: "#18181b" },
  // Text field fill and border: `--input-bg-color`, `--input-border-color`.
  { property: "--input-fill", light: "#ffffff", dark: "#121212" },
  { property: "--input-border", light: "#ebebeb", dark: "#2e2e2e" },
];

/** The chrome's colours as CSS custom properties, for Light or Dark mode. */
export function chromeColors(dark: boolean): Record<string, string> {
  return Object.fromEntries(colors.map((c) => [c.property, dark ? c.dark : c.light]));
}

/** Shows `element` (the page's root) and everything in it in the chrome's colours of Light or Dark mode, form controls and scroll bars included. */
export function applyChromeColors(
  element: { style: { setProperty(name: string, value: string): void } },
  dark: boolean,
): void {
  for (const [property, value] of Object.entries(chromeColors(dark))) {
    element.style.setProperty(property, value);
  }
  element.style.setProperty("color-scheme", dark ? "dark" : "light");
}
