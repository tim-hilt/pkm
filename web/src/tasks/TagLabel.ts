import { make } from "../Dom";

/** A Tag's name in its Tag Color (`#rrggbb`); Default takes the surrounding grey. */
export function tagLabel(name: string, color: string | undefined): HTMLSpanElement {
  const label = make("span", "tag", [name]);
  if (color) label.style.color = color;
  return label;
}
