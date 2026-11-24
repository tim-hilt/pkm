import type { Editor } from "../../core/Core";

/** Asks for a line of text with `initial` filled in; null if cancelled. `window.prompt` satisfies it. */
export type Prompt = (message: string, initial: string) => string | null;

/** The number in `text` if it can be a measure in units: finite and above zero. `,` counts as the decimal separator. */
export function parseUnit(text: string): number | undefined {
  const value = Number(text.trim().replace(",", "."));
  return Number.isFinite(value) && value > 0 ? value : undefined;
}

/**
 * "Set unit…": asks what the Selection's width (a line's length) reads as,
 * filled in with its current measure, and makes it so (one undo step).
 * Asks again for a value that is not above zero, until cancelled.
 * Returns whether the Unit was set; false without asking if the Selection
 * cannot be measured.
 */
export function askUnit(editor: Editor, prompt: Prompt = (m, i) => window.prompt(m, i)): boolean {
  const measure = editor.selectionMeasure();
  if (measure === undefined) return false;
  let text = measure.toFixed(2);
  for (;;) {
    const answer = prompt("What the selected width, or length of a line, reads as", text);
    if (answer === null) return false;
    const value = parseUnit(answer);
    if (value !== undefined) return editor.setUnit(value);
    text = answer;
  }
}
