import { userEvent } from "vitest/browser";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { MemoryStorage } from "../storage/MemoryStorage";
import { NoteFiles } from "./NoteFiles";
import { NotesModel } from "./NotesModel";
import { NotesView } from "./NotesView";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

const views: NotesView[] = [];
afterEach(() => views.splice(0).forEach((view) => view.element.remove()));

describe("NotesView", () => {
  it("shows the open Note as Sync merged it, and typing keeps that", async () => {
    const storage = new MemoryStorage();
    const model = new NotesModel(await NoteFiles.open(storage));
    const view = new NotesView(model);
    document.body.append(view.element);
    views.push(view);
    model.add();
    const id = model.openId!;
    model.save(id, "Plan", "mine\n");
    await model.saving;

    // Another Device's line, merged in by Sync.
    const path = `Notes/${id}.md`;
    const file = new TextDecoder().decode(await storage.read(path));
    await storage.write(path, new TextEncoder().encode(file.replace("mine\n", "mine\ntheirs\n")));
    await model.syncDidChange();

    const body = view.element.querySelector<HTMLElement>('[aria-label="Note"]')!;
    expect(body.textContent).toBe("mine\ntheirs\n");
    body.focus();
    await userEvent.keyboard("more");
    expect(model.note(id)?.body).toContain("theirs");
  });

  it("narrows the list with the Tag menu, which is hidden while no Note has a Tag", async () => {
    const model = new NotesModel(await NoteFiles.open(new MemoryStorage()));
    const view = new NotesView(model);
    document.body.append(view.element);
    views.push(view);
    const menu = view.element.querySelector<HTMLSelectElement>('select[aria-label="Tag"]')!;
    expect(menu.hidden).toBe(true);
    model.add();
    model.save(model.openId!, "Tagged", "");
    await model.setTags(model.openId!, ["work"]);
    model.add();
    model.save(model.openId!, "Plain", "");
    model.open(undefined);
    expect(menu.hidden).toBe(false);
    expect([...menu.options].map((option) => option.text)).toEqual(["All Notes", "work"]);
    await userEvent.selectOptions(menu, "work");
    expect(view.element.querySelectorAll(".note-row")).toHaveLength(1);
  });
});
