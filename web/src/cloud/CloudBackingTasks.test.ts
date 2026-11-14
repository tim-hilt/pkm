import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, Planning, loadCore } from "../core/Core";
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
    server.fetch,
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
  const files = await TaskFiles.open(core, device.storage);
  files.store.create(title, Planning.Active, 4, []);
  await files.save();
  device.cloud.localChange();
}

async function titles(device: Device): Promise<string[]> {
  const files = await TaskFiles.open(core, device.storage);
  const rows = files.store.tasks(Planning.Active);
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

    expect([...server.contents("pkm/Tasks").keys()]).toEqual(
      expect.arrayContaining([
        expect.stringMatching(/^[a-z0-9]{12}\.md$/),
        expect.stringMatching(/\.log$/),
      ]),
    );

    await sync(tablet);
    expect(await titles(tablet)).toEqual(["Buy milk"]);
    expect(tablet.tasksChanged).toBe(1);
  });

  it("Syncs the Tags to the server and another Device, and tells it", async () => {
    const browser = await device();
    const tablet = await device();
    const files = await TaskFiles.open(core, browser.storage);
    files.store.create("Buy milk", Planning.Active, 4, []);
    const [row] = files.store.tasks(Planning.Active);
    files.store.addTag(row.id, "errand");
    row.free();
    await files.save();
    browser.cloud.localChange();
    await settle();

    expect([...server.contents("pkm/Tags").keys()]).toEqual(["tags.json"]);

    await sync(tablet);
    const store = (await TaskFiles.open(core, tablet.storage)).store;
    expect(store.suggestTags("", []).matches).toEqual(["errand"]);
    expect(tablet.tasksChanged).toBeGreaterThan(0);
  });

  it("doesn't tell of History files alone, which the Tasks don't read", async () => {
    const tablet = await device();
    server.put("pkm/Tasks", "bbbbbbbbbbbb-2025-01.log", new TextEncoder().encode("x\n"));
    await sync(tablet);
    expect(await tablet.storage.files("Tasks/.history")).toHaveLength(1);
    expect(tablet.tasksChanged).toBe(0);
  });

  it("tells the other tabs when one saved Task files", async () => {
    const tabs = new MemoryTabGroup();
    const one = await device(tabs);
    const two = await device(tabs);
    one.cloud.tasksSaved();
    expect([one.tasksChanged, two.tasksChanged]).toEqual([0, 1]);
  });

  it("transfers the History of a Device as it is", async () => {
    const browser = await device();
    const tablet = await device();
    await createTask(browser, "Buy milk");
    await settle();
    await sync(tablet);

    const [log] = await browser.storage.files("Tasks/.history");
    const path = `Tasks/.history/${log.name}`;
    expect(text(await tablet.storage.read(path))).toBe(text(await browser.storage.read(path)));
  });
});
