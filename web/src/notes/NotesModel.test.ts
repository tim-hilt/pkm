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
    expect(model.notes.map((row) => row.id)).toEqual([id]);
    model.open(undefined);
    await model.saving;
    expect(model.notes).toEqual([]);
    expect(model.newId).toBeUndefined();
    expect(await saved()).toBe(0);
  });

  it("saves edits as typed and lists them", async () => {
    model.add();
    const id = model.openId!;
    model.save(id, "A *title*", "- first line\n");
    model.open(undefined);
    await model.saving;
    expect(model.notes.map((row) => [row.title, row.preview])).toEqual([
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
    expect(model.notes.map((row) => row.id)).toEqual([stub]);
    expect(await saved()).toBe(1);
  });
});

describe("Tags", () => {
  it("are set in order, listed, kept and known to the app, and Remove leaves them known", async () => {
    model.add();
    const id = model.openId!;
    await model.setTags(id, ["work", "home"]);
    expect(model.note(id)?.tags).toEqual(["work", "home"]);
    await model.setTags(id, ["home", "play"]);
    expect(model.note(id)?.tags).toEqual(["home", "play"]);
    expect(model.notes[0]?.tags).toEqual(["home", "play"]);
    await model.saving;
    const reopened = new NotesModel(await NoteFiles.open(storage));
    expect(reopened.notes[0]?.tags).toEqual(["home", "play"]);
    const known = new TextDecoder().decode((await storage.read("Tags/tags.json"))!);
    expect(known).toContain("work");
    await model.setTags(id, []);
    expect(model.notes[0]?.tags).toEqual([]);
  });

  it("Delete a Tag takes it off every Note", async () => {
    model.add();
    const first = model.openId!;
    model.save(first, "First", "");
    await model.setTags(first, ["work", "home"]);
    model.add();
    const second = model.openId!;
    model.save(second, "Second", "");
    await model.setTags(second, ["work"]);
    model.removeTagEverywhere("work");
    expect(model.note(first)?.tags).toEqual(["home"]);
    expect(model.note(second)?.tags).toEqual([]);
  });

  it("keep a Tag the Tasks added to the shared file", async () => {
    model.add();
    const id = model.openId!;
    await storage.write(
      "Tags/tags.json",
      new TextEncoder().encode('{"tags":{"mine":{"updated":"2020-01-01T00:00:00Z"}}}'),
    );
    await model.setTags(id, ["new"]);
    await model.saving;
    const known = new TextDecoder().decode((await storage.read("Tags/tags.json"))!);
    expect(known).toContain("mine");
    expect(known).toContain("new");
  });
});

describe("Copy Note", () => {
  it("is the Title, then the Body after a blank line", () => {
    model.add();
    const id = model.openId!;
    model.save(id, "Plan", "one\ntwo");
    expect(model.copyText(id)).toBe("Plan\n\none\ntwo");
    model.save(id, "", "one");
    expect(model.copyText(id)).toBe("one");
  });
});

describe("Sync", () => {
  it("closes the open Note that Sync Deleted, and counts the change", async () => {
    model.add();
    const id = model.openId!;
    model.save(id, "Gone", "text");
    await model.saving;
    const syncs = model.syncs;
    await storage.remove(`Notes/${id}.md`);
    await model.syncDidChange();
    expect(model.openId).toBeUndefined();
    expect(model.notes).toEqual([]);
    expect(model.syncs).toBe(syncs + 1);
  });

  it("lists Notes Sync wrote, but not those in a subdirectory", async () => {
    const note = (id: string) =>
      new TextEncoder().encode(`---\nid: ${id}\ntitle: T\nupdated: 2020-01-01T00:00:00Z\n---\n`);
    await storage.write("Notes/aaaaaaaaaaaa.md", note("aaaaaaaaaaaa"));
    await storage.write("Notes/Work/bbbbbbbbbbbb.md", note("bbbbbbbbbbbb"));
    await model.syncDidChange();
    expect(model.notes.map((row) => row.id)).toEqual(["aaaaaaaaaaaa"]);
    await model.saving;
    expect(await storage.files("Notes/Work")).toHaveLength(1);
  });
});

describe("Images", () => {
  const png = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2]);
  const imagesIn = async () =>
    (await storage.files("Notes")).map((file) => file.name).filter((name) => !name.endsWith(".md"));

  /** A Note showing a new Image, as a pasted picture would. */
  const withImage = () => {
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
    expect(await imagesIn()).toEqual([reference]);
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
    expect(await imagesIn()).toEqual([reference]);
    model.open(undefined);
    await model.saving;
    expect(await imagesIn()).toEqual([]);
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
    expect(await imagesIn()).toEqual([kept.reference]);
  });

  it("Deletes a Note's Images with it, unless another Note shows them", async () => {
    const own = withImage();
    const shared = model.addImage(own.id, png)!;
    model.save(own.id, "Pic", `![](${own.reference})\n![](${shared})\n`);
    model.add();
    model.save(model.openId!, "Other", `![](${shared})\n`);
    model.delete(own.id);
    await model.saving;
    expect(await imagesIn()).toEqual([shared]);
  });
});

describe("Search", () => {
  it("finds Notes, Title matches first, with a snippet", () => {
    model.add();
    const inBody = model.openId!;
    model.save(inBody, "Plan", "see the Café menu 😀 today");
    model.add();
    const inTitle = model.openId!;
    model.save(inTitle, "CAFE", "");
    model.search("cafe");
    expect(model.results.map((hit) => [hit.id, hit.titleMatched])).toEqual([
      [inTitle, true],
      [inBody, false],
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
});
