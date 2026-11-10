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
const text = (data: Uint8Array | undefined) => new TextDecoder().decode(data);

interface Device {
  storage: MemoryStorage;
  cloud: CloudBacking;
}

async function device(): Promise<Device> {
  const storage = new MemoryStorage();
  const store = new DrawingStore(storage, core);
  const cloud = await CloudBacking.open(core, storage, server.fetch, store);
  await cloud.connect(url, "user", "secret");
  await advance(0);
  return { storage, cloud };
}

/** A Sync step now, as when the tab comes back to the front. */
async function sync(device: Device): Promise<void> {
  device.cloud.setVisible(false);
  device.cloud.setVisible(true);
  await advance(0);
}

const settle = () => advance(10_000);

const state = (comment: string, at: number): Uint8Array =>
  bytes(
    JSON.stringify({
      id: "aaaaaaaaaaaa",
      file_name: "Emma.epub",
      sha256: "aa",
      imported: 1,
      markup: {
        highlights: [
          {
            id: "h",
            anchor: {
              epub: { start_spine: 0, start_offset: 1, end_spine: 0, end_offset: 2 },
            },
            quoted_text: "text",
            color: 1,
            created: 1,
            comment,
          },
        ],
        page_comments: [],
      },
      progress: null,
      changes: { highlight_comments: { h: at } },
    }),
  );

describe("the Books folder", () => {
  it("transfers Book Files unchanged to the server and another Device", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.storage.write("Books/Emma.epub", bytes("PK\u0003\u0004one"));
    await settle();

    expect(text(server.contents("pkm/Books").get("Emma.epub"))).toBe("PK\u0003\u0004one");

    await sync(tablet);
    expect(text(await tablet.storage.read("Books/Emma.epub"))).toBe("PK\u0003\u0004one");
  });

  it("merges the state files of both Devices, the later change winning", async () => {
    const browser = await device();
    const tablet = await device();
    for (const device of [browser, tablet]) {
      await device.storage.write("Books/Emma.epub", bytes("PK\u0003\u0004one"));
    }
    await browser.storage.write("Books/.state/aaaaaaaaaaaa.json", state("old", 10));
    await tablet.storage.write("Books/.state/aaaaaaaaaaaa.json", state("new", 11));

    await settle();
    await sync(tablet);
    await sync(browser);

    for (const device of [browser, tablet]) {
      const merged = JSON.parse(text(await device.storage.read("Books/.state/aaaaaaaaaaaa.json")));
      expect(merged.markup.highlights[0].comment).toBe("new");
    }
    const onServer = JSON.parse(text(server.contents("pkm/Books").get("aaaaaaaaaaaa.json")));
    expect(onServer.markup.highlights[0].comment).toBe("new");
  });

  it("leaves an unparseable state file alone", async () => {
    const browser = await device();
    const tablet = await device();
    const good = state("kept", 10);
    await browser.storage.write("Books/.state/aaaaaaaaaaaa.json", good);
    await tablet.storage.write("Books/.state/aaaaaaaaaaaa.json", bytes("{"));

    await settle();
    await sync(tablet);
    await sync(browser);

    expect(text(server.contents("pkm/Books").get("aaaaaaaaaaaa.json"))).toBe(text(good));
    expect(text(await tablet.storage.read("Books/.state/aaaaaaaaaaaa.json"))).toBe("{");
  });
});
