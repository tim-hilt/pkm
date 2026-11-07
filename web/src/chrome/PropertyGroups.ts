import {
  Edges,
  FillPattern,
  FontSize,
  LineStyle,
  type Properties,
  type Property,
  Sloppiness,
  StrokeWidth,
  TextAlign,
  ZOrder,
} from "../core/Core";

/**
 * The groups of the Properties panel as data: each has a title, its option
 * set (with the icon and the name a screen reader reads), what the editor
 * says is chosen, and the property a pick sets. The panel stacks them; the
 * phone's popovers will pick the ones they need. A port of the Swift
 * `PropertyGroups`.
 */

/** A glyph in a 20 x 20 box, as SVG markup drawn with the current colour. */
export type Icon = { svg: string } | { text: string };

export interface Option<V> {
  value: V;
  name: string;
  icon: Icon;
}

export interface OptionGroup {
  title: string;
  options: readonly Option<number>[];
  /** The chosen option of what the editor shows; `undefined` where mixed. */
  chosen(shown: Properties): number | undefined;
  /** The property a pick of `value` sets. */
  property(value: number): Property;
  /** Whether the group applies to what the editor shows. */
  applies(shown: Properties): boolean;
}

const stroke = (extra = "") => `fill="none" stroke="currentColor"${extra}`;
const svg = (body: string) => ({ svg: body });

const box = { x: 2, y: 2, size: 16 };
const hatching = (crossed: boolean, id: string) => {
  let lines = "";
  for (let offset = -16; offset <= 16; offset += 4.5) {
    lines += `M${box.x + offset} ${box.y + box.size}l16 -16`;
    if (crossed) lines += `M${box.x + offset} ${box.y}l16 16`;
  }
  return svg(
    `<clipPath id="${id}"><rect x="2" y="2" width="16" height="16" rx="3"/></clipPath>` +
      `<path clip-path="url(#${id})" d="${lines}" ${stroke(' stroke-width="1.2"')}/>` +
      `<rect x="2" y="2" width="16" height="16" rx="3" ${stroke(' stroke-width="1.2"')}/>`,
  );
};
const solidFill = svg(`<rect x="2" y="2" width="16" height="16" rx="3" fill="currentColor"/>`);
const width = (w: number) => svg(`<path d="M3 10H17" ${stroke(` stroke-width="${w}"`)}/>`);
const line = (dash: string, round = false) =>
  svg(
    `<path d="M2 10H18" ${stroke(
      ` stroke-width="${round ? 2 : 1.6}" stroke-linecap="${round ? "round" : "butt"}"` +
        (dash ? ` stroke-dasharray="${dash}"` : ""),
    )}/>`,
  );
const scribble = (wobble: number) => {
  const a = wobble * 2 + 1;
  const path = `M2 13C7 ${13 - a * 2} 12 ${7 + a * 2} 18 7`;
  return svg(
    `<path d="${path}" ${stroke(` stroke-width="${1.2 + wobble * 0.5}" stroke-linecap="round"`)}/>` +
      (wobble === 2
        ? `<path d="${path}" transform="translate(0 2.5)" ${stroke(' stroke-width="1" stroke-linecap="round"')}/>`
        : ""),
  );
};
const corner = (radius: number) =>
  svg(
    `<rect x="2" y="2" width="16" height="16" rx="${radius / 2}" ${stroke(' stroke-width="1.6"')}/>`,
  );
const alignment = (lines: [number, number][]) =>
  svg(
    `<path d="${lines.map(([x1, x2], i) => `M${x1} ${5 + i * 5}H${x2}`).join("")}" ${stroke(' stroke-width="1.6" stroke-linecap="round"')}/>`,
  );

function group<V extends number>(
  title: string,
  section: keyof Properties["sections"],
  field: keyof Properties,
  property: (value: V) => Property,
  options: [V, string, Icon][],
): OptionGroup {
  return {
    title,
    options: options.map(([value, name, icon]) => ({ value, name, icon })),
    chosen: (shown) => shown[field] as number | undefined,
    property: (value) => property(value as V),
    applies: (shown) => shown.sections[section],
  };
}

export const fillGroup = group("Fill", "fillStyle", "fillStyle", (fillStyle) => ({ fillStyle }), [
  [FillPattern.Hachure, "Hachure", hatching(false, "hachure")],
  [FillPattern.CrossHatch, "Cross-hatch", hatching(true, "cross-hatch")],
  [FillPattern.Solid, "Solid", solidFill],
]);

export const strokeWidthGroup = group(
  "Stroke width",
  "strokeWidth",
  "strokeWidth",
  (strokeWidth) => ({ strokeWidth }),
  [
    [StrokeWidth.Thin, "Thin", width(1)],
    [StrokeWidth.Medium, "Bold", width(2.5)],
    [StrokeWidth.Bold, "Extra bold", width(4.5)],
  ],
);

export const strokeStyleGroup = group(
  "Stroke style",
  "strokeStyle",
  "strokeStyle",
  (strokeStyle) => ({ strokeStyle }),
  [
    [LineStyle.Solid, "Solid", line("")],
    [LineStyle.Dashed, "Dashed", line("4 3")],
    [LineStyle.Dotted, "Dotted", line("0.1 3", true)],
  ],
);

export const sloppinessGroup = group(
  "Sloppiness",
  "sloppiness",
  "sloppiness",
  (sloppiness) => ({ sloppiness }),
  [
    [Sloppiness.Architect, "Architect", scribble(0)],
    [Sloppiness.Artist, "Artist", scribble(1)],
    [Sloppiness.Cartoonist, "Cartoonist", scribble(2)],
  ],
);

export const edgesGroup = group("Edges", "edges", "edges", (edges) => ({ edges }), [
  [Edges.Sharp, "Sharp", corner(0)],
  [Edges.Round, "Round", corner(7)],
]);

export const fontSizeGroup = group("Font size", "text", "fontSize", (fontSize) => ({ fontSize }), [
  [FontSize.Small, "Small", { text: "S" }],
  [FontSize.Medium, "Medium", { text: "M" }],
  [FontSize.Large, "Large", { text: "L" }],
  [FontSize.ExtraLarge, "Extra large", { text: "XL" }],
]);

export const textAlignGroup = group(
  "Text align",
  "text",
  "textAlign",
  (textAlign) => ({ textAlign }),
  [
    [
      TextAlign.Left,
      "Left",
      alignment([
        [3, 17],
        [3, 12],
        [3, 15],
      ]),
    ],
    [
      TextAlign.Center,
      "Center",
      alignment([
        [3, 17],
        [6, 14],
        [4.5, 15.5],
      ]),
    ],
    [
      TextAlign.Right,
      "Right",
      alignment([
        [3, 17],
        [8, 17],
        [5, 17],
      ]),
    ],
  ],
);

/** The option groups in the order of the panel, between the colours and opacity (the text groups share the font section). */
export const optionGroups: readonly OptionGroup[] = [
  fillGroup,
  strokeWidthGroup,
  strokeStyleGroup,
  sloppinessGroup,
  edgesGroup,
  fontSizeGroup,
  textAlignGroup,
];

/** A button that acts on the Selection. */
export interface ActionButton {
  name: string;
  icon: Icon;
}

const arrowIcon = (body: string) =>
  svg(
    `<path d="${body}" ${stroke(' stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"')}/>`,
  );

export const layerButtons: readonly (ActionButton & { how: ZOrder })[] = [
  { name: "Send to Back", how: ZOrder.ToBack, icon: arrowIcon("M10 3v10M6 9l4 4 4-4M4 17h12") },
  { name: "Send Backward", how: ZOrder.Backward, icon: arrowIcon("M10 4v11M6 11l4 4 4-4") },
  { name: "Bring Forward", how: ZOrder.Forward, icon: arrowIcon("M10 16V5M6 9l4-4 4 4") },
  { name: "Bring to Front", how: ZOrder.ToFront, icon: arrowIcon("M10 17V7M6 11l4-4 4 4M4 3h12") },
];

export const duplicateButton: ActionButton = {
  name: "Duplicate",
  icon: arrowIcon("M7 7h9v9H7zM13 7V4H4v9h3"),
};
export const deleteButton: ActionButton = {
  name: "Delete",
  icon: arrowIcon("M4 6h12M8 6V4h4v2M6 6l1 10h6l1-10"),
};
