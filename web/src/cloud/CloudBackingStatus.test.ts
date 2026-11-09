import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
import { CloudBacking, ConnectFailed, statusText } from "./CloudBacking";
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
const bytes = (text: string) => new TextEncoder().encode(text);
const drawing = (id: string) =>
  bytes(
    `{"type":"excalidraw","version":2,"elements":[{"id":"${id}","type":"rectangle","version":1,"versionNonce":7,"index":"a0","isDeleted":false,"x":1}],"appState":{},"files":{}}`,
  );

async function device() {
  const storage = new MemoryStorage();
  const store = new DrawingStore(storage, core);
  const cloud = await CloudBacking.open(core, storage, server.fetch, store);
  store.onLocalChange = () => cloud.localChange();
  const progress: string[] = [];
  cloud.onSyncChange(({ progress: p }) => {
    if (p) progress.push(`${p.done} of ${p.total}`);
  });
  return { storage, store, cloud, progress };
}

describe("status", () => {
  it("is not synced yet before the first step, then up to date with the time", async () => {
    const { cloud } = await device();
    expect(cloud.syncReport.status).toBeUndefined();
    await cloud.connect(url, "user", "secret");
    await advance(0);
    expect(cloud.syncReport).toEqual({
      status: "upToDate",
      progress: undefined,
      lastSync: Date.now(),
    });
    expect(statusText(cloud.syncReport, "WebDAV")).toBe("Up to date");
  });

  it("is worded as in the native app", () => {
    const text = (status: Parameters<typeof statusText>[0]) => statusText(status, "WebDAV");
    expect(text({})).toBe("Not synced yet");
    expect(text({ status: "syncing" })).toBe("Syncing");
    expect(text({ status: "syncing", progress: { done: 3, total: 40 } })).toBe(
      "Syncing 3 of 40 Drawings",
    );
    expect(text({ status: "offline" })).toBe("Offline");
    expect(text({ status: "accessLost" })).toBe("Access lost");
    expect(text({ status: "full" })).toBe("WebDAV is full");
  });

  it("counts the Drawings that transfer, and clears the progress after", async () => {
    for (const name of ["A", "B", "C"])
      server.put("pkm/Drawings", `${name}.excalidraw`, drawing(name));
    const { cloud, progress } = await device();
    await cloud.connect(url, "user", "secret");
    await advance(0);
    expect(progress).toContain("0 of 3");
    expect(progress.at(-1)).toBe("3 of 3");
    expect(cloud.syncReport.progress).toBeUndefined();
    expect(cloud.syncReport.status).toBe("upToDate");
  });
});

describe("access lost", () => {
  it("shows until Reconnect, which resumes Sync", async () => {
    const { cloud, store } = await device();
    await cloud.connect(url, "user", "secret");
    await advance(0);
    server.rejectCredentials = true;
    await store.add(drawing("A"), "Sketch");
    await advance(5_000);
    expect(cloud.syncReport.status).toBe("accessLost");
    expect(cloud.hasProblem).toBe(true);
    const before = server.requests.length;
    await advance(120_000);
    expect(server.requests.length).toBe(before);

    server.rejectCredentials = false;
    await expect(cloud.reconnect("wrong")).rejects.toBeInstanceOf(ConnectFailed);
    expect(cloud.syncReport.status).toBe("accessLost");
    await cloud.reconnect("secret");
    await advance(0);
    expect(cloud.syncReport.status).toBe("upToDate");
    expect(cloud.hasProblem).toBe(false);
    expect(server.contents("pkm/Drawings").has("Sketch.excalidraw")).toBe(true);
  });
});

describe("full", () => {
  it("goes on downloading, and clears once uploads succeed", async () => {
    const { cloud, store } = await device();
    await cloud.connect(url, "user", "secret");
    await advance(0);
    server.full = true;
    server.put("pkm/Drawings", "Remote.excalidraw", drawing("R"));
    await store.add(drawing("A"), "Local");
    await advance(5_000);
    expect(cloud.syncReport.status).toBe("full");
    expect(cloud.hasProblem).toBe(true);
    expect((await store.list()).map((e) => e.name).sort()).toEqual(["Local", "Remote"]);
    expect(server.contents("pkm/Drawings").has("Local.excalidraw")).toBe(false);

    server.full = false;
    await advance(10_000);
    expect(cloud.syncReport.status).toBe("upToDate");
    expect(server.contents("pkm/Drawings").has("Local.excalidraw")).toBe(true);
  });
});

describe("offline", () => {
  it("backs off and Syncs at once when the browser is online again", async () => {
    const { cloud } = await device();
    await cloud.connect(url, "user", "secret");
    await advance(0);
    let tries = 0;
    const fetching = server.fetch;
    const counting = async (input: string | URL, init?: RequestInit) => {
      tries++;
      return fetching(input, init);
    };
    const storage = new MemoryStorage();
    const store = new DrawingStore(storage, core);
    const other = await CloudBacking.open(core, storage, counting, store);
    await other.connect(url, "user", "secret");
    await advance(0);
    server.offline = true;
    tries = 0;
    await advance(10_000);
    await advance(20_000);
    await advance(40_000);
    await advance(60_000);
    const failing = tries;
    expect(other.syncReport.status).toBe("offline");
    // Polling every 10 s would have tried 13 times in 130 s; back-off tries far fewer.
    expect(failing).toBeLessThanOrEqual(5);
    server.offline = false;
    other.setOnline(false);
    other.setOnline(true);
    await advance(0);
    expect(other.syncReport.status).toBe("upToDate");
  });
});
