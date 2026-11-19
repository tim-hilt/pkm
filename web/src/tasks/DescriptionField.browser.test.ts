import { userEvent } from "vitest/browser";
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { loadCore } from "../core/Core";
import { isApplePlatform } from "../drawings/input/KeyboardShortcuts";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { DescriptionField } from "./DescriptionField";
import { DescriptionImages, type ImageFiles } from "./DescriptionImages";
import { caretOffset, placeCaret, selectRange } from "./TextCaret";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

const fields: DescriptionField[] = [];
afterEach(() => fields.splice(0).forEach((field) => field.element.remove()));

function open(text: string, images?: DescriptionImages): DescriptionField {
  const field = new DescriptionField(text, images);
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
    // Both fence lines, each with its line break.
    const fence = () =>
      [...field.element.querySelectorAll(".md-fence .md-marker")].map((marker) => [
        marker.textContent,
        marker.classList.contains("md-shown"),
      ]);
    await moveTo(field, 6);
    expect(fence()).toEqual([
      ["```\n", true],
      ["```\n", true],
    ]);
    await moveTo(field, 15);
    expect(fence()).toEqual([
      ["```\n", false],
      ["```\n", false],
    ]);
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
    const mod = isApplePlatform(navigator.platform) ? "Meta" : "Control";
    const press = (key: string) => userEvent.keyboard(`{${mod}>}${key}{/${mod}}`);
    await press("b");
    expect(field.value).toBe("a **b** c");
    await press("i");
    expect(field.value).toBe("a ***b*** c");
    await press("i");
    await press("b");
    expect(field.value).toBe("a b c");
    await press("e");
    expect(field.value).toBe("a `b` c");
  });

  describe("Images", () => {
    const isPng = (bytes: Uint8Array) => bytes[0] === 0x89 && bytes[1] === 0x50;

    /** A store that keeps PNGs only, as the core does besides JPEG, GIF and WebP. */
    function pngStore(clipboard?: Pick<Clipboard, "write">) {
      const kept = new Map<string, Uint8Array>();
      const files: ImageFiles = {
        addImage(bytes) {
          if (!isPng(bytes)) return undefined;
          const reference = `a${kept.size}.png`;
          kept.set(reference, bytes);
          return reference;
        },
        readImage: async (reference) => kept.get(reference),
      };
      return { kept, images: new DescriptionImages(files, undefined, clipboard) };
    }

    const png = async () => {
      const canvas = new OffscreenCanvas(3, 2);
      canvas.getContext("2d")!.fillRect(0, 0, 3, 2);
      return new Uint8Array(
        await (await canvas.convertToBlob({ type: "image/png" })).arrayBuffer(),
      );
    };

    /** A 1x1 BMP, a format the core doesn't keep. */
    const bmp = () =>
      new Uint8Array([
        0x42, 0x4d, 0x3a, 0, 0, 0, 0, 0, 0, 0, 0x36, 0, 0, 0, 0x28, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0,
        1, 0, 0x18, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff,
        0, 0, 0,
      ]);

    const pasting = (bytes: Uint8Array, type: string) => {
      const data = new DataTransfer();
      data.items.add(new File([bytes as Uint8Array<ArrayBuffer>], "picture", { type }));
      return new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true });
    };

    const shown = async (field: DescriptionField) => {
      const image = () => field.element.querySelector("img")!;
      await vi.waitFor(() => expect(image().naturalWidth).toBeGreaterThan(0));
      return image();
    };

    it("pastes a PNG as a reference on its own line and shows the Image", async () => {
      const { kept, images } = pngStore();
      const field = open("a", images);
      const bytes = await png();
      await moveTo(field, 1);
      field.element.dispatchEvent(pasting(bytes, "image/png"));
      await vi.waitFor(() => expect(field.value).toBe("a\n![](a0.png)\n"));
      expect(kept.get("a0.png")).toEqual(bytes);
      expect(images.takeAdded()).toEqual(["a0.png"]);
      expect(images.takeAdded()).toEqual([]);
      await moveTo(field, 0);
      const image = await shown(field);
      expect([image.naturalWidth, image.naturalHeight]).toEqual([3, 2]);
      expect(image.alt).toBe("Image");
    });

    it("stores a picture of another format as PNG", async () => {
      const { kept, images } = pngStore();
      const field = open("", images);
      field.element.dispatchEvent(pasting(bmp(), "image/bmp"));
      await vi.waitFor(() => expect(field.value).toBe("![](a0.png)\n"));
      expect(isPng(kept.get("a0.png")!)).toBe(true);
    });

    it("shows the reference while the caret is on the line, and the Image still", async () => {
      const { images } = pngStore();
      const field = open("", images);
      field.element.dispatchEvent(pasting(await png(), "image/png"));
      await vi.waitFor(() => expect(field.value).toBe("![](a0.png)\n"));
      const line = () => field.element.querySelector<HTMLElement>(".md-image")!;
      await moveTo(field, 3);
      expect(line().classList.contains("md-shown")).toBe(true);
      expect(line().querySelector("img")).not.toBeNull();
      await moveTo(field, 13);
      expect(line().classList.contains("md-shown")).toBe(false);
    });

    it("copies an Image to the clipboard from its context menu", async () => {
      const written: ClipboardItem[] = [];
      const { images } = pngStore({ write: async (items) => void written.push(...items) });
      const field = open("", images);
      const bytes = await png();
      field.element.dispatchEvent(pasting(bytes, "image/png"));
      await vi.waitFor(() => expect(field.value).toBe("![](a0.png)\n"));
      const image = await shown(field);
      const opened = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      image.dispatchEvent(opened);
      expect(opened.defaultPrevented).toBe(true);
      const entry = [...document.querySelectorAll<HTMLElement>(".row-menu-item")].find(
        (item) => item.textContent === "Copy Image to Clipboard",
      );
      entry!.click();
      await vi.waitFor(() => expect(written).toHaveLength(1));
      const blob = await written[0].getType("image/png");
      expect(new Uint8Array(await blob.arrayBuffer())).toEqual(bytes);
    });

    it("leaves the context menu of the text to the browser", async () => {
      const { images } = pngStore();
      const field = open("text", images);
      const opened = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
      field.element.dispatchEvent(opened);
      expect(opened.defaultPrevented).toBe(false);
    });

    it("ignores a picture that is none, and pictures when there is no store", async () => {
      const { images } = pngStore();
      const field = open("", images);
      field.element.dispatchEvent(pasting(new Uint8Array([1, 2, 3]), "image/png"));
      const plain = open("x");
      const event = pasting(await png(), "image/png");
      plain.element.dispatchEvent(event);
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(field.value).toBe("");
      expect(plain.value).toBe("x");
    });
  });
});
