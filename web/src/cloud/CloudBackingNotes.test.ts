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

/** Writes a Note on the Device, as the app does, and saves it. */
async function createNote(device: Device, title: string, body: string): Promise<string> {
  const files = await NoteFiles.open(device.storage);
  const note = files.store.create("");
  const { id } = note;
  note.free();
  files.store.edit(id, title, body).free();
  await files.save();
  device.cloud.localChange();
  return id;
}

async function titles(device: Device): Promise<string[]> {
  const files = await NoteFiles.open(device.storage);
  const rows = files.store.notes("");
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

  it("tells the other tabs when one saved Note files", async () => {
    const tabs = new MemoryTabGroup();
    const one = await device(tabs);
    const two = await device(tabs);
    one.cloud.notesSaved();
    expect([one.notesChanged, two.notesChanged]).toEqual([0, 1]);
  });
});
