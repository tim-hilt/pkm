import { readFileSync } from "node:fs";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { NoteFiles } from "./NoteFiles";
import { NotesModel } from "./NotesModel";

const generated = new URL("../../generated/", import.meta.url);
beforeAll(async () => {
  await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

let storage: MemoryStorage;
let model: NotesModel;
beforeEach(async () => {
  storage = new MemoryStorage();
  model = new NotesModel(await NoteFiles.open(storage));
});

const saved = async () => (await storage.files("Notes")).length;

describe("the Notes list", () => {
  it("opens a new Note with its Title to focus; leaving it empty discards it", async () => {
    model.add();
    const id = model.openId!;
    expect(model.newId).toBe(id);
    expect(model.rows.map((row) => row.id)).toEqual([id]);
    model.open(undefined);
    await model.saving;
    expect(model.rows).toEqual([]);
    expect(model.newId).toBeUndefined();
    expect(await saved()).toBe(0);
  });

  it("saves edits as typed and lists them", async () => {
    model.add();
    const id = model.openId!;
    model.save(id, "A *title*", "- first line\n");
    model.open(undefined);
    await model.saving;
    expect(model.rows.map((row) => [row.title, row.preview])).toEqual([
      ["A *title*", "first line"],
    ]);
    expect(model.note(id)?.body).toBe("- first line\n");
    expect(await saved()).toBe(1);
  });

  it("Deletes a Note, asking first only when its Body has text", async () => {
    model.add();
    const stub = model.openId!;
    model.save(stub, "Stub", "");
    expect(model.confirmsDelete(stub)).toBe(false);
    model.add();
    const written = model.openId!;
    model.save(written, "", "text");
    expect(model.confirmsDelete(written)).toBe(true);
    model.delete(written);
    await model.saving;
    expect(model.openId).toBeUndefined();
    expect(model.rows.map((row) => row.id)).toEqual([stub]);
    expect(await saved()).toBe(1);
  });
});
