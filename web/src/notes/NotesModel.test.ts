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

  it("keeps a Note open that Sync Moved to another Folder", async () => {
    model.add();
    const id = model.openId!;
    model.save(id, "Moved by Sync", "text");
    await model.saving;
    // What a Sync step writes: the new file first, then the old one goes.
    const bytes = (await storage.read(`Notes/${id}.md`))!;
    await storage.write(`Notes/Work/${id}.md`, bytes);
    await storage.remove(`Notes/${id}.md`);
    await model.syncDidChange();
    expect(model.openId).toBe(id);
    expect(model.note(id)?.title).toBe("Moved by Sync");
    model.select("Work");
    expect(model.rows.map((row) => row.id)).toEqual([id]);
  });

  it("leaves a Folder Sync Deleted for the one around it, and closes the open Note it took", async () => {
    model.addFolder("Work", "");
    model.addFolder("Plans", "Work");
    model.add();
    model.save(model.openId!, "Gone with it", "text");
    await model.saving;
    const syncs = model.syncs;
    // What a Sync step does for a Folder Deleted elsewhere.
    await storage.remove("Notes/Work/Plans");
    await model.syncDidChange();
    expect(model.folder).toBe("Work");
    expect(model.openId).toBeUndefined();
    expect(model.rows).toEqual([]);
    expect(model.syncs).toBe(syncs + 1);
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

  it("keeps the open Note when the selected Folder is selected again", () => {
    model.addFolder("Work", "");
    model.add();
    const id = model.openId!;
    model.select("Work");
    expect(model.openId).toBe(id);
  });

  it("pushes the selected Folder over the outline and back", () => {
    expect(model.showsFolder).toBe(false);
    model.select("");
    expect(model.showsFolder).toBe(true);
    model.leaveFolder();
    expect(model.showsFolder).toBe(false);
  });
});

describe("Images", () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2]);
  const imagesIn = async (folder: string) =>
    (await storage.files(folder)).map((file) => file.name).filter((name) => !name.endsWith(".md"));

  /** A Note in `folder` showing a new Image, as a pasted picture would. */
  const withImage = (folder = "") => {
    model.select(folder);
    model.add();
    const id = model.openId!;
    const reference = model.addImage(id, png)!;
    model.save(id, "Pic", `![](${reference})\n`);
    return { id, reference };
  };

  it("adds an Image beside the Note, which reads back", async () => {
    const { id, reference } = withImage();
    expect(await model.readImage(id, reference)).toEqual(png);
    await model.saving;
    expect(await imagesIn("Notes")).toEqual([reference]);
    expect(model.addImage(id, new Uint8Array([1, 2, 3]))).toBeUndefined();
  });

  it("reads an Image after a reopen", async () => {
    const { id, reference } = withImage();
    await model.saving;
    const reopened = new NotesModel(await NoteFiles.open(storage));
    expect(await reopened.readImage(id, reference)).toEqual(png);
  });

  it("releases an Image removed from the Body when the Note is left", async () => {
    const { id, reference } = withImage();
    model.save(id, "Pic", "no picture\n");
    await model.saving;
    expect(await imagesIn("Notes")).toEqual([reference]);
    model.open(undefined);
    await model.saving;
    expect(await imagesIn("Notes")).toEqual([]);
  });

  it("releases an Image the Note started with, and keeps one it still shows", async () => {
    const kept = withImage();
    const dropped = model.addImage(kept.id, png)!;
    model.save(kept.id, "Pic", `![](${kept.reference})\n![](${dropped})\n`);
    model.open(undefined);
    model.open(kept.id);
    model.save(kept.id, "Pic", `![](${kept.reference})\n`);
    model.open(undefined);
    await model.saving;
    expect(await imagesIn("Notes")).toEqual([kept.reference]);
  });

  it("Deletes a Note's Images with it, and a Folder's with the Folder", async () => {
    model.addFolder("Work", "");
    const note = withImage("Work");
    const loose = withImage("");
    await model.saving;
    expect(await imagesIn("Notes/Work")).toEqual([note.reference]);
    model.deleteFolder("Work");
    model.delete(loose.id);
    await model.saving;
    expect(await imagesIn("Notes/Work")).toEqual([]);
    expect(await imagesIn("Notes")).toEqual([]);
  });

  it("Moves the Images only the Note shows, and leaves shared ones", async () => {
    model.addFolder("Work", "");
    const own = withImage("");
    const shared = model.addImage(own.id, png)!;
    model.save(own.id, "Pic", `![](${own.reference})\n![](${shared})\n`);
    model.add();
    const other = model.openId!;
    model.save(other, "Other", `![](${shared})\n`);
    model.open(undefined);
    model.moveNote(own.id, "Work");
    await model.saving;
    expect(await imagesIn("Notes/Work")).toEqual([own.reference]);
    expect(await imagesIn("Notes")).toEqual([shared]);
  });

  it("Moves Images saved before, with their Folder", async () => {
    model.addFolder("Work", "");
    const { reference } = withImage("Work");
    await model.saving;
    const reopened = new NotesModel(await NoteFiles.open(storage));
    reopened.addFolder("Plans", "");
    reopened.moveFolder("Work", "Plans");
    await reopened.saving;
    expect(await imagesIn("Notes/Plans/Work")).toEqual([reference]);
    expect(await imagesIn("Notes/Work")).toEqual([]);
  });
});

describe("Search", () => {
  it("finds Notes in any Folder, Title matches first, with Folder and snippet", () => {
    model.addFolder("Work", "");
    model.add();
    const inBody = model.openId!;
    model.save(inBody, "Plan", "see the Café menu 😀 today");
    model.select("");
    model.add();
    const inTitle = model.openId!;
    model.save(inTitle, "CAFE", "");
    model.search("cafe");
    expect(model.results.map((hit) => [hit.id, hit.folder, hit.titleMatched])).toEqual([
      [inTitle, "", true],
      [inBody, "Work", false],
    ]);
    const { snippet, matchStart, matchEnd } = model.results[1]!;
    expect(snippet.slice(matchStart, matchEnd)).toBe("Café");
  });

  it("finds nothing for a blank query, and follows changes while searching", () => {
    model.add();
    const id = model.openId!;
    model.save(id, "Alpha", "");
    model.search("  ");
    expect(model.results).toEqual([]);
    model.search("beta");
    expect(model.results).toEqual([]);
    model.save(id, "Beta", "");
    expect(model.results.map((hit) => hit.id)).toEqual([id]);
    model.search("");
    expect(model.results).toEqual([]);
  });

  it("opens a result in its Folder", () => {
    model.addFolder("Work", "");
    model.add();
    const id = model.openId!;
    model.save(id, "Roadmap", "");
    model.select("");
    model.search("road");
    model.openResult(model.results[0]!);
    expect(model.folder).toBe("Work");
    expect(model.openId).toBe(id);
    expect(model.rows.map((row) => row.id)).toEqual([id]);
  });
});
