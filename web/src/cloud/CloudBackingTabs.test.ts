import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
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
let inFlight: number;
let overlapped: boolean;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  server = new MemoryWebDav(url);
  inFlight = 0;
  overlapped = false;
});
afterEach(() => vi.useRealTimers());

const flush = () => new Promise((resolve) => setImmediate(resolve));
async function advance(ms: number): Promise<void> {
  vi.advanceTimersByTime(ms);
  // Slowed requests take a macrotask each.
  for (let i = 0; i < 300; i++) await flush();
}
const bytes = (text: string) => new TextEncoder().encode(text);
const text = (data: Uint8Array | undefined) => new TextDecoder().decode(data);

/** The server, slowed so that two steps running together would show up as overlapping requests. */
const slowFetch = async (input: string, init?: RequestInit) => {
  // A step of one tab is under way between its first and last request; count requests only.
  inFlight++;
  if (inFlight > 1) overlapped = true;
  await flush();
  try {
    return await server.fetch(input, init);
  } finally {
    inFlight--;
  }
};

interface Tab {
  store: DrawingStore;
  cloud: CloudBacking;
  synced: string[][];
}

/** A tab: its own module and store over the browser's one storage. */
async function tab(storage: MemoryStorage, group: MemoryTabGroup): Promise<Tab> {
  const store = new DrawingStore(storage, core);
  const cloud = await CloudBacking.open(core, storage, slowFetch, store, group.tab());
  store.onLocalChange = () => cloud.localChange();
  const synced: string[][] = [];
  cloud.onSynced((names) => synced.push(names.slice().sort()));
  return { store, cloud, synced };
}

describe("several tabs of one browser", () => {
  it("steps of two tabs never overlap", async () => {
    const storage = new MemoryStorage();
    const group = new MemoryTabGroup();
    const one = await tab(storage, group);
    await one.cloud.connect(url, "user", "secret");
    const two = await tab(storage, group);
    await advance(0);
    await one.store.save(bytes("a"), "A");
    await two.store.save(bytes("b"), "B");
    one.cloud.localChange();
    two.cloud.localChange();
    await advance(5_000);
    await advance(5_000);
    expect(server.contents("pkm").size).toBe(2);
    expect(overlapped).toBe(false);
  });

  it("a change one tab Synced reaches the other's listeners", async () => {
    const storage = new MemoryStorage();
    const group = new MemoryTabGroup();
    const other = new MemoryStorage();
    const phone = await tab(other, new MemoryTabGroup());
    await phone.cloud.connect(url, "user", "secret");
    await phone.store.save(bytes("one"), "Sketch");
    await advance(5_000);

    const one = await tab(storage, group);
    const two = await tab(storage, group);
    await one.cloud.connect(url, "user", "secret");
    await advance(5_000);
    expect(text(await two.store.open("Sketch"))).toBe("one");
    expect(two.synced).toEqual([["Sketch"]]);
    expect(one.synced).toEqual([["Sketch"]]);
  });

  it("Connect in one tab starts Sync in the other", async () => {
    const storage = new MemoryStorage();
    const group = new MemoryTabGroup();
    const one = await tab(storage, group);
    const two = await tab(storage, group);
    await one.cloud.connect(url, "user", "secret");
    await advance(0);
    expect(two.cloud.connection?.server).toBe(url);
    await two.store.save(bytes("x"), "Mine");
    await advance(5_000);
    expect(text(server.contents("pkm").get("Mine.excalidraw"))).toBe("x");
  });

  it("Disconnect in one tab stops Sync in the other", async () => {
    const storage = new MemoryStorage();
    const group = new MemoryTabGroup();
    const one = await tab(storage, group);
    const two = await tab(storage, group);
    await one.cloud.connect(url, "user", "secret");
    await advance(0);
    await one.cloud.disconnect();
    await advance(0);
    expect(two.cloud.connection).toBeUndefined();
    const before = server.requests.length;
    await two.store.save(bytes("x"), "Mine");
    await advance(30_000);
    expect(server.requests.length).toBe(before);
    expect(await two.store.list()).toHaveLength(1);
  });
});
