import { rangeOf } from "./ArticleOffsets";

/** A passage to draw, by UTF-16 offsets into the Article's Markdown. */
export interface Passage {
  start: number;
  end: number;
}

const hex = (rgb: number) => rgb.toString(16).padStart(6, "0");

const found = "reader-found";
const foundCurrent = "reader-found-current";
const foundRules = `
::highlight(${found}) { background-color: rgb(255 150 0 / 0.35); color: inherit; }
::highlight(${foundCurrent}) { background-color: rgb(255 120 0 / 0.85); color: inherit; }
`;

/**
 * What the Reader draws over the text of its Article without changing it: Highlights as background
 * colors and Article Search's matches, as CSS custom highlights (`::highlight`), whose ranges
 * the browser keeps following the text. Several Readers must not draw at once: the names are
 * the document's.
 */
export class ArticleInk {
  private readonly style = document.createElement("style");
  /** The names of the Highlight colors drawn now. */
  private colors: string[] = [];

  constructor(private readonly root: HTMLElement) {
    this.style.textContent = foundRules;
    document.head.append(this.style);
  }

  /** Draws the Highlights, each in its own color, in place of those drawn before. */
  showHighlights(highlights: (Passage & { rgb: number })[]): void {
    for (const name of this.colors) CSS.highlights.delete(name);
    const byColor = new Map<number, Passage[]>();
    for (const { rgb, start, end } of highlights) {
      byColor.set(rgb, [...(byColor.get(rgb) ?? []), { start, end }]);
    }
    let rules = foundRules;
    this.colors = [];
    for (const [rgb, passages] of byColor) {
      const name = `reader-ink-${hex(rgb)}`;
      // Half see-through, so that the text stays readable in light and Dark mode.
      rules += `::highlight(${name}) { background-color: #${hex(rgb)}8c; color: inherit; }\n`;
      this.colors.push(name);
      this.draw(name, passages, 1);
    }
    this.style.textContent = rules;
  }

  /** Marks every match of Article Search, the `current` one stronger; none marks nothing. */
  showFound(matches: Passage[], current: number): void {
    this.draw(found, matches, 2);
    this.draw(foundCurrent, matches.slice(current, current + 1), 3);
  }

  dispose(): void {
    for (const name of [...this.colors, found, foundCurrent]) CSS.highlights.delete(name);
    this.style.remove();
  }

  private draw(name: string, passages: Passage[], priority: number): void {
    if (passages.length === 0) return void CSS.highlights.delete(name);
    const highlight = new Highlight(...passages.map((p) => rangeOf(this.root, p.start, p.end)));
    highlight.priority = priority;
    CSS.highlights.set(name, highlight);
  }
}
