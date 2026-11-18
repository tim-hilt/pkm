import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { Closing, type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "../tasks/TaskFiles";
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

const text = (data: Uint8Array | undefined) => new TextDecoder().decode(data);

interface Device {
  storage: MemoryStorage;
  cloud: CloudBacking;
  tasksChanged: number;
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
  const device = { storage, cloud, tasksChanged: 0 };
  cloud.onTasksChanged(() => device.tasksChanged++);
  if (tabs) return device;
  await cloud.connect(url, "user", "secret");
  await advance(0);
  return device;
}

/** Creates a Task on the Device, as the app does, and saves it. */
async function createTask(device: Device, title: string): Promise<void> {
  const files = await TaskFiles.open(device.storage);
  files.store.create(title, 4, []);
  await files.save();
  device.cloud.localChange();
}

async function titles(device: Device): Promise<string[]> {
  const files = await TaskFiles.open(device.storage);
  const rows = files.store.tasks();
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

describe("the Tasks folders", () => {
  it("Syncs a Task to the server and another Device, and tells it", async () => {
    const browser = await device();
    const tablet = await device();
    await createTask(browser, "Buy milk");
    await settle();

    expect([...server.contents("pkm/Tasks").keys()]).toEqual([
      expect.stringMatching(/^[a-z0-9]{12}\.md$/),
    ]);

    await sync(tablet);
    expect(await titles(tablet)).toEqual(["Buy milk"]);
    expect(tablet.tasksChanged).toBe(1);
  });

  it("reads a Task another Device Closed since, past the browser's HTTP cache", async () => {
    const browser = await device();
    const tablet = await device();
    await createTask(tablet, "Buy milk");
    await settle();
    await sync(browser);
    expect(await titles(browser)).toEqual(["Buy milk"]);

    const files = await TaskFiles.open(tablet.storage);
    const [row] = files.store.tasks();
    files.store.close(row.id, Closing.Done);
    row.free();
    files.store.commit();
    await files.save();
    tablet.cloud.localChange();
    await settle();

    await sync(browser);
    expect(await titles(browser)).toEqual([]);
  });

  it("Syncs the Tags to the server and another Device, and tells it", async () => {
    const browser = await device();
    const tablet = await device();
    const files = await TaskFiles.open(browser.storage);
    files.store.create("Buy milk", 4, []);
    const [row] = files.store.tasks();
    files.store.addTag(row.id, "errand");
    row.free();
    await files.save();
    browser.cloud.localChange();
    await settle();

    expect([...server.contents("pkm/Tags").keys()]).toEqual(["tags.json"]);

    await sync(tablet);
    const store = (await TaskFiles.open(tablet.storage)).store;
    expect(store.suggestTags("", []).matches).toEqual(["errand"]);
    expect(tablet.tasksChanged).toBeGreaterThan(0);
  });

  it("Syncs the Filters to the server and another Device, and tells it", async () => {
    const browser = await device();
    const tablet = await device();
    const filter =
      "---\nid: aaaaaaaaaaaa\ntitle: Work\nquery: '@work'\nfavorite: false\nposition: 1\nupdated: 2025-01-01T00:00:00Z\n---\n";
    await browser.storage.write("Filters/aaaaaaaaaaaa.md", new TextEncoder().encode(filter));
    browser.cloud.localChange();
    await settle();

    expect([...server.contents("pkm/Filters").keys()]).toEqual(["aaaaaaaaaaaa.md"]);

    await sync(tablet);
    expect(text(await tablet.storage.read("Filters/aaaaaaaaaaaa.md"))).toBe(filter);
    expect(tablet.tasksChanged).toBeGreaterThan(0);
  });

  it("tells the other tabs when one saved Task files", async () => {
    const tabs = new MemoryTabGroup();
    const one = await device(tabs);
    const two = await device(tabs);
    one.cloud.tasksSaved();
    expect([one.tasksChanged, two.tasksChanged]).toEqual([0, 1]);
  });
});
