import { beforeAll, expect, it } from "vitest";
import { TaskQuickAdd, loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { QuickAddField } from "./QuickAddField";
import type { TasksModel } from "./TasksModel";
import { selectionOffsets } from "./TextCaret";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

it("ignores a pasted picture, which would be no text", async () => {
  const quickAdd = new TaskQuickAdd();
  const field = new QuickAddField({ allTags: [] } as unknown as TasksModel, quickAdd);
  document.body.append(field.element);
  field.focus();
  const data = new DataTransfer();
  data.items.add(new File([new Uint8Array([0x89, 0x50])], "picture", { type: "image/png" }));
  field.element.dispatchEvent(
    new ClipboardEvent("paste", { clipboardData: data, bubbles: true, cancelable: true }),
  );
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(field.title).toBe("");
  field.element.remove();
});

it("puts the caret at the end of a prefilled text", () => {
  const quickAdd = new TaskQuickAdd();
  quickAdd.replace(0, 0, "@bug ");
  const field = new QuickAddField({ allTags: [] } as unknown as TasksModel, quickAdd);
  document.body.append(field.element);
  field.focus();
  const editor = field.element.querySelector<HTMLElement>(".tasks-quick-input")!;
  expect(selectionOffsets(editor)).toEqual({ start: 5, end: 5 });
  field.element.remove();
});
