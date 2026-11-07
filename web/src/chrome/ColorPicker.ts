import {
  type ColorKind,
  colorFromHex,
  colorName,
  displayColor,
  grid,
  isTransparent,
  shadesOf,
} from "./ColorPalette";

const hexDigits = (color: string | undefined) => (color?.startsWith("#") ? color.slice(1) : "");

/**
 * A colour row of the Properties panel: the five top picks and, at the end,
 * a button showing the current colour that opens the full picker below the
 * row (the palette grid, the shades of the current colour and a hex code
 * field). Every swatch is a labelled button with a touch target of at least
 * 36 px. A port of the Swift `ColorPickerView`.
 */
export class ColorPicker {
  readonly element = document.createElement("div");
  private readonly swatches: { button: HTMLButtonElement; color: string }[] = [];
  private readonly more = document.createElement("button");
  private readonly full = document.createElement("div");
  private readonly shades = document.createElement("div");
  private readonly hex = document.createElement("input");
  private readonly invalid = document.createElement("div");
  private selected: string | undefined;
  private shown = false;

  constructor(
    kind: ColorKind,
    title: string,
    topPicks: readonly string[],
    private readonly pick: (color: string) => void,
    private readonly isDark: () => boolean,
  ) {
    this.element.className = "color-picker";
    const row = this.row();
    for (const color of topPicks) row.append(this.swatch(color, 22));
    const divider = document.createElement("div");
    divider.className = "color-divider";
    this.more.type = "button";
    this.more.className = "color-more";
    this.more.setAttribute("aria-label", `More colours, ${title}`);
    this.more.setAttribute("aria-expanded", "false");
    this.more.addEventListener("click", () => {
      this.shown = !this.shown;
      this.full.hidden = !this.shown;
      this.more.setAttribute("aria-expanded", String(this.shown));
      this.more.classList.toggle("open", this.shown);
    });
    row.append(divider, this.more);

    this.full.className = "color-full";
    this.full.hidden = true;
    this.full.append(this.heading("Colors"));
    const colors = grid(kind);
    for (let r = 0; r < 3; r++) {
      const gridRow = this.row();
      for (const color of colors.slice(r * 5, r * 5 + 5)) gridRow.append(this.swatch(color, 26));
      this.full.append(gridRow);
    }
    this.shades.className = "color-row";
    this.full.append(this.heading("Shades"), this.shades, this.heading("Hex code"));

    const field = document.createElement("div");
    field.className = "hex-field";
    const hash = document.createElement("span");
    hash.textContent = "#";
    this.hex.type = "text";
    this.hex.setAttribute("aria-label", "Hex code");
    this.hex.autocomplete = "off";
    this.hex.spellcheck = false;
    this.hex.autocapitalize = "off";
    this.hex.addEventListener("keydown", (event) => {
      if (event.key === "Enter") this.commitHex();
    });
    this.hex.addEventListener("blur", () => this.commitHex());
    field.append(hash, this.hex);
    this.invalid.className = "hex-invalid";
    this.invalid.textContent = "Not a valid color";
    this.invalid.hidden = true;
    this.full.append(field, this.invalid);
    this.element.append(row, this.full);
  }

  /** Shows `selected`, the colour of the Selection (`undefined` where its Elements differ). */
  update(selected: string | undefined): void {
    const dark = this.isDark();
    this.selected = selected;
    for (const { button, color } of this.swatches) this.mark(button, color, selected);
    const current = selected ?? "transparent";
    this.paint(this.more, current, dark);
    this.more.setAttribute("aria-description", selected ? colorName(selected) : "");

    if (!this.full.hidden) {
      const colors = shadesOf(selected);
      this.shades.replaceChildren(
        ...(colors?.map((color) => this.swatch(color, 26, false)) ?? [this.noShades()]),
      );
      if (document.activeElement !== this.hex) {
        this.hex.value = hexDigits(selected);
        this.invalid.hidden = true;
      }
    }
  }

  private commitHex(): void {
    if (this.hex.value === hexDigits(this.selected)) {
      this.invalid.hidden = true;
      return;
    }
    const color = colorFromHex(this.hex.value);
    this.invalid.hidden = color !== undefined || this.hex.value === "";
    if (color) this.pick(color);
  }

  private swatch(color: string, size: number, remember = true): HTMLButtonElement {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "swatch";
    button.style.setProperty("--size", `${size}px`);
    button.setAttribute("aria-label", colorName(color));
    button.addEventListener("click", () => this.pick(color));
    this.paint(button, color, this.isDark());
    this.mark(button, color, this.selected);
    if (remember) this.swatches.push({ button, color });
    return button;
  }

  private paint(button: HTMLElement, color: string, dark: boolean): void {
    button.style.setProperty("--swatch", displayColor(color, dark));
    button.classList.toggle("transparent", isTransparent(color));
    if (button === this.more) button.style.setProperty("--size", "26px");
  }

  private mark(button: HTMLElement, color: string, selected: string | undefined): void {
    const on = color === selected;
    button.setAttribute("aria-pressed", String(on));
    button.classList.toggle("selected", on);
  }

  private noShades(): HTMLElement {
    const note = document.createElement("div");
    note.className = "panel-caption";
    note.textContent = "No shades available for this color";
    return note;
  }

  private row(): HTMLElement {
    const row = document.createElement("div");
    row.className = "color-row";
    return row;
  }

  private heading(title: string): HTMLElement {
    const heading = document.createElement("div");
    heading.className = "panel-caption";
    heading.textContent = title;
    return heading;
  }
}
