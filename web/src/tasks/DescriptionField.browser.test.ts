import { userEvent } from "vitest/browser";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { DescriptionField } from "./DescriptionField";
import { caretOffset, placeCaret, selectRange } from "./TextCaret";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

const fields: DescriptionField[] = [];
afterEach(() => fields.splice(0).forEach((field) => field.element.remove()));

function open(text: string): DescriptionField {
  const field = new DescriptionField(text);
  document.body.append(field.element);
  fields.push(field);
  field.focus();
  return field;
}

/** Moves the caret to the UTF-16 `offset` and lets the field react. */
async function moveTo(field: DescriptionField, offset: number) {
  placeCaret(field.element, offset);
  await new Promise((resolve) => setTimeout(resolve));
}

/** Whether the marker of the heading shows (the CSS hides the others). */
const markerShown = (field: DescriptionField) =>
  field.element.querySelector(".md-marker")!.classList.contains("md-shown");

describe("DescriptionField", () => {
  it("keeps the caret and the text while typing, across lines", async () => {
    const field = open("");
    await userEvent.keyboard("# Hi{Enter}some **bold** text");
    expect(field.value).toBe("# Hi\nsome **bold** text");
    expect(caretOffset(field.element)).toBe(field.value.length);
    await moveTo(field, 12);
    await userEvent.keyboard("X");
    expect(field.value).toBe("# Hi\nsome **Xbold** text");
    expect(caretOffset(field.element)).toBe(13);
  });

  it("renders a line once the caret leaves it, and shows its markers while it is there", async () => {
    const field = open("# Title\nbody");
    await moveTo(field, 3);
    expect(markerShown(field)).toBe(true);
    await moveTo(field, 10);
    expect(markerShown(field)).toBe(false);
    expect(field.value).toBe("# Title\nbody");
  });

  it("shows a fenced code block raw as a whole while the caret is inside it", async () => {
    const field = open("```\ncode\n```\n**x**");
    const fence = () => field.element.querySelector(".md-fence .md-marker")!;
    await moveTo(field, 6);
    expect(fence().classList.contains("md-shown")).toBe(true);
    await moveTo(field, 15);
    expect(fence().classList.contains("md-shown")).toBe(false);
  });

  it("opens a rendered link on a press, and leaves a raw one to edit", async () => {
    const opened: string[] = [];
    const original = window.open;
    window.open = (url) => (opened.push(String(url)), null);
    try {
      const field = open("[a](https://example.com)\nplain");
      await moveTo(field, 28);
      field.element
        .querySelector("a")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
      expect(opened).toEqual(["https://example.com"]);
      await moveTo(field, 2);
      expect(field.element.querySelector("a[data-href]")).toBeNull();
    } finally {
      window.open = original;
    }
  });

  it("keeps a trailing line break editable", async () => {
    const field = open("a");
    await userEvent.keyboard("{End}{Enter}");
    expect(field.value).toBe("a\n");
    await userEvent.keyboard("b");
    expect(field.value).toBe("a\nb");
  });
  it("continues a list on Return, ends it on an empty item and numbers on", async () => {
    const field = open("");
    await userEvent.keyboard("3. a{Enter}b{Enter}");
    expect(field.value).toBe("3. a\n4. b\n5. ");
    await userEvent.keyboard("{Enter}");
    expect(field.value).toBe("3. a\n4. b\n");
    expect(caretOffset(field.element)).toBe(10);
  });

  it("indents and outdents items with Tab and Shift-Tab", async () => {
    const field = open("- a\n- b");
    await moveTo(field, 7);
    await userEvent.keyboard("{Tab}");
    expect(field.value).toBe("- a\n  - b");
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(field.value).toBe("- a\n- b");
    await userEvent.keyboard("{Shift>}{Tab}{/Shift}");
    expect(field.value).toBe("- a\n- b");
  });

  it("undoes a list continuation", async () => {
    const field = open("- a");
    await userEvent.keyboard("{End}{Enter}");
    expect(field.value).toBe("- a\n- ");
    await userEvent.keyboard("{Control>}z{/Control}");
    expect(field.value).toBe("- a");
    await userEvent.keyboard("{Control>}{Shift>}z{/Shift}{/Control}");
    expect(field.value).toBe("- a\n- ");
  });

  it("toggles Inline formatting on the selection with Mod-B, Mod-I and Mod-E", async () => {
    const field = open("a b c");
    selectRange(field.element, 2, 3);
    await userEvent.keyboard("{Control>}b{/Control}");
    expect(field.value).toBe("a **b** c");
    await userEvent.keyboard("{Control>}i{/Control}");
    expect(field.value).toBe("a ***b*** c");
    await userEvent.keyboard("{Control>}i{/Control}{Control>}b{/Control}");
    expect(field.value).toBe("a b c");
    await userEvent.keyboard("{Control>}e{/Control}");
    expect(field.value).toBe("a `b` c");
  });
});
