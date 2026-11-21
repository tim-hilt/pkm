import { userEvent } from "vitest/browser";
import { beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { pickTagsDialog } from "./TagPicker";
import type { TasksModel } from "./TasksModel";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

describe("the quick Tag picker", () => {
  const model = { allTags: [{ name: "home" }, { name: "work" }] } as unknown as TasksModel;
  const toggled: string[] = [];
  const open = () => {
    toggled.length = 0;
    pickTagsDialog(document.body, model, ["work"], { kind: "one", toggle: (t) => toggled.push(t) });
    return document.querySelector<HTMLDialogElement>("dialog.tasks-picker")!;
  };
  const box = (dialog: HTMLDialogElement, tag: string) =>
    [...dialog.querySelectorAll("label")].find((l) => l.textContent?.includes(tag))!;

  it("toggles one tapped Tag, checked or not, and closes", async () => {
    let dialog = open();
    expect(dialog.querySelector(".tasks-confirm")).toBeNull();
    await userEvent.click(box(dialog, "home"));
    expect(toggled).toEqual(["home"]);
    expect(dialog.isConnected).toBe(false);

    dialog = open();
    await userEvent.click(box(dialog, "work"));
    expect(toggled).toEqual(["work"]);
    expect(dialog.isConnected).toBe(false);
  });

  it("toggles a Tag created from search, and closes", async () => {
    const dialog = open();
    await userEvent.type(dialog.querySelector("input[type=search]")!, "fresh{Enter}");
    expect(toggled).toEqual(["fresh"]);
    expect(dialog.isConnected).toBe(false);
  });

  it("changes nothing on Cancel", async () => {
    const dialog = open();
    await userEvent.click(
      [...dialog.querySelectorAll("button")].find((b) => b.textContent === "Cancel")!,
    );
    expect(toggled).toEqual([]);
    expect(dialog.isConnected).toBe(false);
  });
});
