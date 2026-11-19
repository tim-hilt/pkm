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

describe("Folders", () => {
  const names = () => model.folders.map((folder) => `${"-".repeat(folder.depth)}${folder.name}`);

  it("makes Folders, selects them, and puts new Notes in the selected one", async () => {
    model.addFolder("Work", "");
    model.addFolder("Plans", "Work");
    expect(model.folder).toBe("Work/Plans");
    expect(names()).toEqual(["Work", "-Plans"]);
    model.add();
    model.save(model.openId!, "Roadmap", "");
    model.select("Work");
    expect(model.rows).toEqual([]);
    model.select("Work/Plans");
    expect(model.rows.map((row) => row.title)).toEqual(["Roadmap"]);
    await model.saving;
    const reopened = new NotesModel(await NoteFiles.open(storage));
    expect(reopened.folders.map((folder) => folder.path)).toEqual(["Work", "Work/Plans"]);
    reopened.select("Work/Plans");
    expect(reopened.rows.map((row) => row.title)).toEqual(["Roadmap"]);
  });

  it("keeps empty Folders, and forgets Deleted and renamed ones", async () => {
    model.addFolder("Empty", "");
    model.addFolder("Old", "");
    model.renameFolder("Old", "New");
    model.addFolder("Gone", "");
    model.deleteFolder("Gone");
    await model.saving;
    expect(await storage.folders("Notes")).toEqual(expect.arrayContaining(["Empty", "New"]));
    expect(await storage.folders("Notes")).toHaveLength(2);
  });

  it("refuses names with the core's reason", () => {
    model.addFolder("Work", "");
    expect(() => model.addFolder("work", "")).toThrow("already there");
    expect(() => model.addFolder("a/b", "")).toThrow("/");
    expect(() => model.addFolder(" ", "")).toThrow("empty");
    expect(() => model.renameFolder("Work", ".x")).toThrow("dot");
  });

  it("keeps the selection on a renamed or Moved Folder, and counts before Deleting", () => {
    model.addFolder("A", "");
    model.addFolder("B", "A");
    model.addFolder("C", "");
    model.select("A/B");
    model.renameFolder("A", "Z");
    expect(model.folder).toBe("Z/B");
    model.moveFolder("Z", "C");
    expect(model.folder).toBe("C/Z/B");
    expect(() => model.moveFolder("C", "C/Z")).toThrow("into itself");
    expect(model.summary("C")).toBe("This Deletes 0 Notes and 2 Folders.");
    model.deleteFolder("C/Z");
    expect(model.folder).toBe("C");
    expect(model.summary("C")).toBe("It is empty.");
  });

  it("Moves a Note, which stays open, and saves it in its new Folder", async () => {
    model.addFolder("Work", "");
    model.select("");
    model.add();
    const id = model.openId!;
    model.save(id, "Moved", "text");
    model.moveNote(id, "Work");
    expect(model.openId).toBe(id);
    expect(model.rows).toEqual([]);
    expect(() => model.moveNote(id, "Nowhere")).toThrow();
    await model.saving;
    expect(await storage.files("Notes")).toEqual([]);
    expect((await storage.files("Notes/Work")).map((file) => file.name)).toEqual([`${id}.md`]);
    model.select("Work");
    expect(model.rows.map((row) => row.id)).toEqual([id]);
  });

  it("Deleting a Folder removes its Notes, and the open one closes", async () => {
    model.addFolder("Work", "");
    model.add();
    model.save(model.openId!, "Doomed", "x");
    model.deleteFolder("Work");
    expect(model.openId).toBeUndefined();
    await model.saving;
    expect(await storage.folders("Notes")).toEqual([]);
    expect(await storage.files("Notes/Work")).toEqual([]);
  });

  it("pushes the selected Folder over the outline and back", () => {
    expect(model.showsFolder).toBe(false);
    model.select("");
    expect(model.showsFolder).toBe(true);
    model.leaveFolder();
    expect(model.showsFolder).toBe(false);
  });
});
