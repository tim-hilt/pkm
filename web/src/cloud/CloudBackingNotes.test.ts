import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { NoteFiles } from "../notes/NoteFiles";
import { MemoryStorage } from "../storage/MemoryStorage";
import { CloudBacking } from "./CloudBacking";
import { MemoryTabGroup } from "./MemoryTabs";
import { MemoryWebDav } from "./MemoryWebDav";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const url = "https://dav.example/dav";
let server: MemoryWebDav;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  server = new MemoryWebDav(url);
});
afterEach(() => vi.useRealTimers());

const flush = () => new Promise((resolve) => setImmediate(resolve));

async function advance(ms: number): Promise<void> {
  vi.advanceTimersByTime(ms);
  await flush();
}

interface Device {
  storage: MemoryStorage;
  cloud: CloudBacking;
  notesChanged: number;
}

/** A Device, Connected; or a tab of the browser of `tabs`, not Connected yet. */
async function device(tabs?: MemoryTabGroup): Promise<Device> {
  const storage = new MemoryStorage();
  const cloud = await CloudBacking.open(
    core,
    storage,
    server.cachingFetch(),
    new DrawingStore(storage, core),
    tabs?.tab(),
  );
  const device = { storage, cloud, notesChanged: 0 };
  cloud.onNotesChanged(() => device.notesChanged++);
  if (tabs) return device;
  await cloud.connect(url, "user", "secret");
  await advance(0);
  return device;
}

/** Writes a Note in `folder` (a path, empty for the top level) on the Device, as the app does, and saves it. */
async function createNote(
  device: Device,
  title: string,
  body: string,
  folder = "",
): Promise<string> {
  const files = await NoteFiles.open(device.storage);
  const note = files.store.create(folder);
  const { id } = note;
  note.free();
  files.store.edit(id, title, body).free();
  await files.save();
  device.cloud.localChange();
  return id;
}

async function titles(device: Device, folder = ""): Promise<string[]> {
  const files = await NoteFiles.open(device.storage);
  const rows = files.store.notes(folder);
  const found = rows.map((row) => row.title);
  rows.forEach((row) => row.free());
  return found;
}

/** A Sync step now, as when the tab comes back to the front. */
async function sync(device: Device): Promise<void> {
  device.cloud.setVisible(false);
  device.cloud.setVisible(true);
  await advance(0);
}

const settle = () => advance(10_000);

describe("the Notes folder", () => {
  it("Syncs a Note to the server and another Device, and tells it", async () => {
    const browser = await device();
    const tablet = await device();
    await createNote(browser, "Plan", "one\n");
    await settle();

    expect([...server.contents("pkm/Notes").keys()]).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    await sync(tablet);
    expect(await titles(tablet)).toEqual(["Plan"]);
    expect(tablet.notesChanged).toBe(1);
  });

  it("reads a Note another Device edited since", async () => {
    const browser = await device();
    const tablet = await device();
    const id = await createNote(tablet, "Plan", "one\n");
    await settle();
    await sync(browser);

    const files = await NoteFiles.open(tablet.storage);
    files.store.edit(id, "Roadmap", "one\n").free();
    await files.save();
    tablet.cloud.localChange();
    await settle();

    await sync(browser);
    expect(await titles(browser)).toEqual(["Roadmap"]);
  });

  it("Syncs a Note in a nested Folder and an empty Folder, in the same places", async () => {
    const browser = await device();
    const tablet = await device();
    const files = await NoteFiles.open(browser.storage);
    files.store.createFolder("", "Work");
    files.store.createFolder("Work", "Plans");
    files.store.createFolder("", "Later");
    await files.save();
    await createNote(browser, "Roadmap", "one\n", "Work/Plans");
    await settle();

    expect([...server.contents("pkm/Notes/Work/Plans").keys()]).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);
    expect(server.hasFolder("pkm/Notes/Later")).toBe(true);

    await sync(tablet);
    expect(await titles(tablet, "Work/Plans")).toEqual(["Roadmap"]);
    const folders = await NoteFiles.open(tablet.storage);
    expect(folders.store.folders().map((row) => row.path)).toContain("Later");
    expect(tablet.notesChanged).toBe(1);
  });

  it("Deletes a Folder on the other Device, with the Note edited there meanwhile", async () => {
    const browser = await device();
    const tablet = await device();
    const id = await createNote(browser, "Roadmap", "one\n");
    const files = await NoteFiles.open(browser.storage);
    files.store.createFolder("", "Work");
    files.store.moveNote(id, "Work");
    await files.save();
    await settle();
    await sync(tablet);
    expect(await titles(tablet, "Work")).toEqual(["Roadmap"]);

    files.store.deleteFolder("Work");
    await files.save();
    const edited = await NoteFiles.open(tablet.storage);
    edited.store.edit(id, "Roadmap", "one\ntwo\n").free();
    await edited.save();
    for (const one of [browser, tablet, browser]) await sync(one);

    expect(server.hasFolder("pkm/Notes/Work")).toBe(false);
    expect(server.contents("pkm/Notes/Work").size).toBe(0);
    expect((await NoteFiles.open(tablet.storage)).store.folders()).toHaveLength(0);
  });

  it("tells the other tabs when one saved Note files", async () => {
    const tabs = new MemoryTabGroup();
    const one = await device(tabs);
    const two = await device(tabs);
    one.cloud.notesSaved();
    expect([one.notesChanged, two.notesChanged]).toEqual([0, 1]);
  });
});
