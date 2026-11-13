import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, Planning, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
import { TaskFiles } from "../tasks/TaskFiles";
import { CloudBacking } from "./CloudBacking";
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
  tasksSynced: number;
}

async function device(): Promise<Device> {
  const storage = new MemoryStorage();
  const cloud = await CloudBacking.open(
    core,
    storage,
    server.fetch,
    new DrawingStore(storage, core),
  );
  const device = { storage, cloud, tasksSynced: 0 };
  cloud.onTasksSynced(() => device.tasksSynced++);
  await cloud.connect(url, "user", "secret");
  await advance(0);
  return device;
}

/** Creates a Task on the Device, as the app does, and saves it. */
async function createTask(device: Device, title: string): Promise<void> {
  const files = await TaskFiles.open(core, device.storage);
  files.store.create(title, Planning.Active);
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
    expect(tablet.tasksSynced).toBe(1);
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
