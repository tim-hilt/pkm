import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
import { CloudBacking } from "./CloudBacking";
import { MemoryWebDav } from "./MemoryWebDav";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("excalidraw_native_web_bg.wasm", generated)));
});

const url = "https://dav.example/dav";
let server: MemoryWebDav;

beforeEach(() => {
  // Timers and the clock are ours; microtasks and `setImmediate` stay real, so `flush` lets everything pending finish.
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  server = new MemoryWebDav(url);
});
afterEach(() => vi.useRealTimers());

const flush = () => new Promise((resolve) => setImmediate(resolve));

/** Moves the clock, running what falls due, and lets every step it starts finish. */
async function advance(ms: number): Promise<void> {
  vi.advanceTimersByTime(ms);
  await flush();
}

const bytes = (text: string) => new TextEncoder().encode(text);
const text = (data: Uint8Array | undefined) => new TextDecoder().decode(data);

/** A Device: its Drawing store on its own storage, and its Cloud Backing. */
interface Device {
  storage: MemoryStorage;
  store: DrawingStore;
  cloud: CloudBacking;
  synced: string[][];
}

async function device(connect = true): Promise<Device> {
  const storage = new MemoryStorage();
  const store = new DrawingStore(storage);
  const cloud = await CloudBacking.open(core, storage, server.fetch, store);
  store.onLocalChange = () => cloud.localChange();
  const synced: string[][] = [];
  cloud.onSynced((names) => synced.push(names.slice().sort()));
  if (connect) await cloud.connect(url, "user", "secret");
  await advance(0);
  return { storage, store, cloud, synced };
}

/** A Sync step now, as when the tab comes back to the front. */
async function sync(...devices: Device[]): Promise<void> {
  for (const { cloud } of devices) {
    cloud.setVisible(false);
    cloud.setVisible(true);
    await advance(0);
  }
}

/** Long enough for a debounced step and for the files to settle. */
const settle = (ms = 5_000) => advance(ms);

const cloudFile = (name: string) => server.contents("ExcalidrawNative").get(`${name}.excalidraw`);
const names = async ({ store }: Device) =>
  (await store.list())
    .map((e) => e.name)
    .slice()
    .sort();

function element(id: string, version: number, index: string, x: number): string {
  return `{"id":"${id}","type":"rectangle","version":${version},"versionNonce":7,"index":"${index}","isDeleted":false,"x":${x}}`;
}
const drawing = (...elements: string[]) =>
  bytes(
    `{"type":"excalidraw","version":2,"elements":[${elements.join(",")}],"appState":{},"files":{}}`,
  );
const elementsOf = (data: Uint8Array | undefined) =>
  (JSON.parse(text(data)).elements as { id: string; version: number }[]).map(
    (e) => `${e.id}@${e.version}`,
  );

describe("changes made on one side only", () => {
  it("a new Drawing reaches the server and another Device's Gallery", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(bytes("one"), "Sketch");
    await settle();
    expect(text(cloudFile("Sketch"))).toBe("one");
    await sync(tablet);
    expect(await names(tablet)).toEqual(["Sketch"]);
    expect(text(await tablet.store.open("Sketch"))).toBe("one");
    expect(tablet.synced).toEqual([["Sketch"]]);
  });

  it("an edit reaches the other Device in both directions", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(bytes("one"), "Sketch");
    await settle();
    await sync(tablet);

    await browser.store.save(bytes("two!"), "Sketch");
    await settle();
    await sync(tablet);
    expect(text(await tablet.store.open("Sketch"))).toBe("two!");

    await tablet.store.save(bytes("three"), "Sketch");
    await settle();
    await sync(browser);
    expect(text(cloudFile("Sketch"))).toBe("three");
    expect(text(await browser.store.open("Sketch"))).toBe("three");
  });

  it("a rename reaches the other Device in both directions", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(bytes("one"), "Old");
    await settle();
    await sync(tablet);

    await browser.store.rename("Old", "New");
    await settle();
    expect([...server.contents("ExcalidrawNative").keys()]).toEqual(["New.excalidraw"]);
    await sync(tablet);
    expect(await names(tablet)).toEqual(["New"]);

    await tablet.store.rename("New", "Newer");
    await settle();
    await sync(browser);
    expect(await names(browser)).toEqual(["Newer"]);
  });

  it("a delete reaches the other Device in both directions", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(bytes("a"), "A");
    await browser.store.save(bytes("b"), "B");
    await settle();
    await sync(tablet);

    await browser.store.delete("A");
    await settle();
    expect(cloudFile("A")).toBeUndefined();
    await sync(tablet);
    expect(await names(tablet)).toEqual(["B"]);

    await tablet.store.delete("B");
    await settle();
    await sync(browser);
    expect(await names(browser)).toEqual([]);
    expect(server.contents("ExcalidrawNative").size).toBe(0);
  });

  it("a Drawing the server changed shows in the Gallery with a new modification time", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(bytes("one"), "Sketch");
    await settle();
    await sync(tablet);
    const before = (await tablet.store.list())[0].modified;
    await advance(1_000);
    await browser.store.save(bytes("two"), "Sketch");
    await settle();
    await sync(tablet);
    expect((await tablet.store.list())[0].modified).toBeGreaterThan(before);
  });

  it("merges edits made on both sides apart, Element by Element", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)), "S");
    await settle();
    await sync(tablet);

    tablet.cloud.setOnline(false);
    browser.cloud.setOnline(false);
    await browser.store.save(drawing(element("a", 2, "a0", 10), element("b", 1, "a1", 0)), "S");
    await tablet.store.save(drawing(element("a", 1, "a0", 0), element("b", 3, "a1", 30)), "S");
    await settle();
    browser.cloud.setOnline(true);
    await advance(0);
    tablet.cloud.setOnline(true);
    await advance(0);
    await sync(browser);

    const merged = ["a@2", "b@3"];
    expect(elementsOf(cloudFile("S"))).toEqual(merged);
    expect(elementsOf(await browser.store.open("S"))).toEqual(merged);
    expect(elementsOf(await tablet.store.open("S"))).toEqual(merged);
  });

  it("an import, a duplicate and an autosave count as changes made here", async () => {
    const browser = await device();
    await browser.store.add(bytes("imported"), "Imported");
    await browser.store.duplicate("Imported");
    await settle();
    expect([...server.contents("ExcalidrawNative").keys()].sort()).toEqual([
      "Imported copy.excalidraw",
      "Imported.excalidraw",
    ]);
  });
});

describe("an idle poll", () => {
  it("costs one listing and transfers nothing", async () => {
    const browser = await device();
    await browser.store.save(bytes("one"), "Sketch");
    await browser.store.save(bytes("two"), "Other");
    await settle(40_000); // until the files are known to be unchanged
    server.requests.length = 0;
    await advance(10_000);
    expect(server.requests).toHaveLength(1);
    expect(server.requests[0]).toMatch(/^PROPFIND /);
  });
});

describe("the schedule", () => {
  it("waits for the debounce after a change, and a burst makes one step", async () => {
    const browser = await device();
    await settle(1_000);
    server.requests.length = 0;
    await browser.store.save(bytes("one"), "A");
    await advance(2_000);
    await browser.store.save(bytes("two"), "A");
    await advance(2_000); // 4 s after the first change, 2 s after the last
    expect(server.requests).toEqual([]);
    await advance(1_500);
    expect(text(cloudFile("A"))).toBe("two");
  });

  it("polls every 10 seconds while shown", async () => {
    await device();
    await advance(1_000);
    server.requests.length = 0;
    await advance(10_000);
    const first = server.requests.length;
    expect(first).toBeGreaterThan(0);
    await advance(10_000);
    expect(server.requests.length).toBeGreaterThan(first);
  });

  it("does nothing while hidden, and syncs at once when shown again", async () => {
    const browser = await device();
    const tablet = await device();
    browser.cloud.setVisible(false);
    await tablet.store.save(bytes("one"), "A");
    await settle();
    tablet.cloud.setVisible(false);
    await browser.store.save(bytes("local"), "B");
    server.requests.length = 0;
    await advance(60_000);
    expect(server.requests).toEqual([]);

    browser.cloud.setVisible(true);
    await advance(0);
    expect(await names(browser)).toEqual(["A", "B"]);
    expect(text(cloudFile("B"))).toBe("local");
  });

  it("does nothing while offline, and syncs at once when online again", async () => {
    const browser = await device();
    await advance(1_000);
    browser.cloud.setOnline(false);
    await browser.store.save(bytes("one"), "A");
    server.requests.length = 0;
    await advance(60_000);
    expect(server.requests).toEqual([]);

    browser.cloud.setOnline(true);
    await advance(0);
    expect(text(cloudFile("A"))).toBe("one");
  });

  it("keeps changes made while the server is unreachable and retries", async () => {
    const browser = await device();
    await advance(1_000);
    server.offline = true;
    await browser.store.save(bytes("one"), "A");
    await settle(30_000);
    expect(cloudFile("A")).toBeUndefined();
    expect(await names(browser)).toEqual(["A"]);
    server.offline = false;
    await advance(60_000);
    expect(text(cloudFile("A"))).toBe("one");
  });
});

describe("Disconnect", () => {
  it("keeps the Drawings, stops Syncing and forgets the state", async () => {
    const browser = await device();
    await browser.store.save(bytes("one"), "A");
    await settle();
    await browser.cloud.disconnect();
    expect(await names(browser)).toEqual(["A"]);
    expect(await browser.storage.files("Cloud/state/markers")).toEqual([]);
    expect(await browser.storage.read("Cloud/connection.json")).toBeUndefined();

    server.requests.length = 0;
    await browser.store.save(bytes("two"), "A");
    await advance(60_000);
    expect(server.requests).toEqual([]);
    expect(text(cloudFile("A"))).toBe("one");
  });

  it("lets Connecting again merge without duplicates", async () => {
    const browser = await device();
    await browser.store.save(bytes("one"), "A");
    await settle();
    await browser.cloud.disconnect();
    await browser.cloud.connect(url, "user", "secret");
    await settle(30_000);
    expect(await names(browser)).toEqual(["A"]);
    expect([...server.contents("ExcalidrawNative").keys()]).toEqual(["A.excalidraw"]);
  });

  it("Syncs again after a restart of the module", async () => {
    const first = await device();
    await first.store.save(bytes("one"), "A");
    await settle();
    const store = new DrawingStore(first.storage);
    const cloud = await CloudBacking.open(core, first.storage, server.fetch, store);
    store.onLocalChange = () => cloud.localChange();
    await store.save(bytes("two"), "A");
    await settle();
    expect(text(cloudFile("A"))).toBe("two");
  });
});
