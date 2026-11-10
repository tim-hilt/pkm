import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
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
  const store = new DrawingStore(storage, core);
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

const cloudFile = (name: string) => server.contents("pkm/Drawings").get(`${name}.excalidraw`);
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
    expect([...server.contents("pkm/Drawings").keys()]).toEqual(["New.excalidraw"]);
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
    expect(server.contents("pkm/Drawings").size).toBe(0);
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
    expect([...server.contents("pkm/Drawings").keys()].sort()).toEqual([
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

describe("Sync state", () => {
  it("is kept per Context folder, so that state from before the folders deletes nothing", async () => {
    const browser = await device(false);
    await browser.store.save(bytes("one"), "A");
    // What the Device remembered of the `pkm` folder, which held the Drawings then.
    await browser.storage.write("Cloud/state/bytes/A.excalidraw", bytes("one"));
    await browser.storage.write("Cloud/state/markers/A.excalidraw", bytes("etag"));
    await browser.cloud.connect(url, "user", "secret");
    await settle();
    expect(await names(browser)).toEqual(["A"]);
    expect(text(cloudFile("A"))).toBe("one");
    expect(await browser.storage.files("Cloud/state/Drawings/markers")).toHaveLength(1);
  });
});

describe("Disconnect", () => {
  it("keeps the Drawings, stops Syncing and forgets the state", async () => {
    const browser = await device();
    await browser.store.save(bytes("one"), "A");
    await settle();
    await browser.cloud.disconnect();
    expect(await names(browser)).toEqual(["A"]);
    expect(await browser.storage.files("Cloud/state/Drawings/markers")).toEqual([]);
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
    expect([...server.contents("pkm/Drawings").keys()]).toEqual(["A.excalidraw"]);
  });

  it("Syncs again after a restart of the module", async () => {
    const first = await device();
    await first.store.save(bytes("one"), "A");
    await settle();
    const store = new DrawingStore(first.storage, core);
    const cloud = await CloudBacking.open(core, first.storage, server.fetch, store);
    store.onLocalChange = () => cloud.localChange();
    await store.save(bytes("two"), "A");
    await settle();
    expect(text(cloudFile("A"))).toBe("two");
  });
});

/** A Drawing with full control of the Elements, `files` and `appState`. */
const full = (elements: string[], files = "{}", appState = "{}") =>
  bytes(
    `{"type":"excalidraw","version":2,"elements":[${elements.join(",")}],"appState":${appState},"files":${files}}`,
  );
const erased = (id: string, version: number, index: string) =>
  element(id, version, index, 0).replace('"isDeleted":false', '"isDeleted":true');
const parsed = (data: Uint8Array | undefined) => JSON.parse(text(data));

/** Two Devices that edited apart from the same Drawing "S", both offline, then back online. */
async function editedApart(
  a: Uint8Array,
  b: Uint8Array,
  base = drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)),
) {
  const browser = await device();
  const tablet = await device();
  await browser.store.save(base, "S");
  await settle();
  await sync(tablet);
  browser.cloud.setOnline(false);
  tablet.cloud.setOnline(false);
  await browser.store.save(a, "S");
  await tablet.store.save(b, "S");
  await settle();
  tablet.cloud.setOnline(true);
  await advance(0);
  browser.cloud.setOnline(true);
  await advance(0);
  await sync(tablet, browser);
  return { browser, tablet };
}

describe("merges", () => {
  it("merges erases", async () => {
    const { browser, tablet } = await editedApart(
      drawing(element("a", 1, "a0", 0), erased("b", 2, "a1")),
      drawing(element("a", 3, "a0", 5), element("b", 1, "a1", 0)),
    );
    for (const data of [
      cloudFile("S"),
      await browser.store.open("S"),
      await tablet.store.open("S"),
    ])
      expect(elementsOf(data)).toEqual(["a@3", "b@2"]);
    expect(parsed(cloudFile("S")).elements[1].isDeleted).toBe(true);
  });

  it("merges images and keeps appState", async () => {
    const { browser, tablet } = await editedApart(
      full(
        [element("a", 1, "a0", 0)],
        '{"f1":{"id":"f1","dataURL":"data:x"}}',
        '{"viewBackgroundColor":"#111111"}',
      ),
      full(
        [element("a", 1, "a0", 0)],
        '{"f2":{"id":"f2","dataURL":"data:y"}}',
        '{"viewBackgroundColor":"#222222"}',
      ),
      full([element("a", 1, "a0", 0)]),
    );
    for (const data of [
      cloudFile("S"),
      await browser.store.open("S"),
      await tablet.store.open("S"),
    ]) {
      const merged = parsed(data);
      expect(Object.keys(merged.files).sort()).toEqual(["f1", "f2"]);
      expect(merged.appState.viewBackgroundColor).toMatch(/^#[12]{6}$/);
    }
    expect(text(await browser.store.open("S"))).toBe(text(await tablet.store.open("S")));
  });

  it("merges three Devices", async () => {
    const browser = await device();
    const tablet = await device();
    const phone = await device();
    await browser.store.save(drawing(element("a", 1, "a0", 0)), "S");
    await settle();
    await sync(tablet, phone);
    for (const d of [browser, tablet, phone]) d.cloud.setOnline(false);
    await browser.store.save(drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)), "S");
    await tablet.store.save(drawing(element("a", 1, "a0", 0), element("c", 1, "a2", 0)), "S");
    await phone.store.save(drawing(element("a", 4, "a0", 9)), "S");
    await settle();
    for (const d of [browser, tablet, phone]) {
      d.cloud.setOnline(true);
      await advance(0);
    }
    await sync(browser, tablet, phone, browser, tablet);
    for (const d of [browser, tablet, phone])
      expect(elementsOf(await d.store.open("S")).sort()).toEqual(["a@4", "b@1", "c@1"]);
  });
});

describe("first Connect", () => {
  it("merges same-named Drawings that share Elements", async () => {
    const other = await device();
    await other.store.save(drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)), "S");
    await settle();
    const browser = await device(false);
    await browser.store.save(drawing(element("a", 1, "a0", 0), element("c", 1, "a2", 0)), "S");
    await browser.cloud.connect(url, "user", "secret");
    await settle();
    expect(await names(browser)).toEqual(["S"]);
    expect(elementsOf(await browser.store.open("S")).sort()).toEqual(["a@1", "b@1", "c@1"]);
    expect(elementsOf(cloudFile("S")).sort()).toEqual(["a@1", "b@1", "c@1"]);
  });

  it("merges same-named empty Drawings", async () => {
    const other = await device();
    await other.store.save(drawing(), "S");
    await settle();
    const browser = await device(false);
    await browser.store.save(drawing(), "S");
    await browser.cloud.connect(url, "user", "secret");
    await settle();
    expect(await names(browser)).toEqual(["S"]);
    expect([...server.contents("pkm/Drawings").keys()]).toEqual(["S.excalidraw"]);
  });

  it("keeps unrelated same-named Drawings, renaming the browser's", async () => {
    const other = await device();
    await other.store.save(drawing(element("x", 1, "a0", 0)), "Sketch");
    await settle();
    const browser = await device(false);
    await browser.store.save(drawing(element("y", 1, "a0", 0)), "Sketch");
    await browser.cloud.connect(url, "user", "secret");
    await settle();
    expect(await names(browser)).toEqual(["Sketch", "Sketch 2"]);
    expect(elementsOf(await browser.store.open("Sketch"))).toEqual(["x@1"]);
    expect(elementsOf(await browser.store.open("Sketch 2"))).toEqual(["y@1"]);
    expect([...server.contents("pkm/Drawings").keys()].sort()).toEqual([
      "Sketch 2.excalidraw",
      "Sketch.excalidraw",
    ]);
    await sync(other);
    expect(await names(other)).toEqual(["Sketch", "Sketch 2"]);
  });

  it("brings every server Drawing to a new browser", async () => {
    const other = await device();
    await other.store.save(bytes("a"), "A");
    await other.store.save(bytes("b"), "B");
    await settle();
    const browser = await device();
    await settle();
    expect(await names(browser)).toEqual(["A", "B"]);
  });
});

describe("edit against delete", () => {
  it("a delete wins over an edit, either way round", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(drawing(element("a", 1, "a0", 0)), "S");
    await browser.store.save(drawing(element("a", 1, "a0", 0)), "T");
    await settle();
    await sync(tablet);
    browser.cloud.setOnline(false);
    tablet.cloud.setOnline(false);
    await browser.store.delete("S");
    await tablet.store.save(drawing(element("a", 2, "a0", 5)), "S");
    await tablet.store.delete("T");
    await browser.store.save(drawing(element("a", 2, "a0", 5)), "T");
    await settle();
    tablet.cloud.setOnline(true);
    await advance(0);
    browser.cloud.setOnline(true);
    await advance(0);
    await sync(tablet, browser, tablet);
    for (const d of [browser, tablet]) expect(await names(d)).toEqual([]);
    expect([...server.contents("pkm/Drawings").keys()]).toEqual([]);
  });
});

describe("healing", () => {
  it("repairs a write lost to another Device's overwrite", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(drawing(element("a", 1, "a0", 0)), "S");
    await settle();
    await sync(tablet);
    await browser.store.save(drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)), "S");
    await settle();
    // Another Device's overwrite lands on the server without the browser's change.
    await tablet.store.save(drawing(element("a", 1, "a0", 0), element("c", 1, "a2", 0)), "S");
    await settle();
    await sync(browser, tablet, browser, tablet);
    for (const d of [browser, tablet])
      expect(elementsOf(await d.store.open("S")).sort()).toEqual(["a@1", "b@1", "c@1"]);
    expect(elementsOf(cloudFile("S")).sort()).toEqual(["a@1", "b@1", "c@1"]);
  });
});

describe("unreadable Drawings", () => {
  it("are left alone on both sides when the cloud's file can't be read", async () => {
    const { browser } = await editedApart(
      drawing(element("a", 2, "a0", 5), element("b", 1, "a1", 0)),
      drawing(element("a", 1, "a0", 0), element("b", 1, "a1", 0)),
    );
    server.put("pkm/Drawings", "S.excalidraw", bytes("{broken"));
    await browser.store.save(drawing(element("a", 3, "a0", 6), element("b", 1, "a1", 0)), "S");
    const local = await browser.store.open("S");
    await settle();
    await sync(browser, browser);
    expect(await browser.store.open("S")).toEqual(local);
    expect(text(cloudFile("S"))).toBe("{broken");
  });

  it("are left alone when the local file can't be read", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.store.save(drawing(element("a", 1, "a0", 0)), "S");
    await settle();
    await sync(tablet);
    const good = cloudFile("S");
    await tablet.store.save(bytes("{broken"), "S");
    await settle();
    await sync(tablet, browser);
    expect(text(await tablet.store.open("S"))).toBe("{broken");
    expect(cloudFile("S")).toEqual(good);
  });
});
