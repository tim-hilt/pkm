import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { BookmarkFiles } from "../bookmarks/BookmarkFiles";
import { BookmarksModel } from "../bookmarks/BookmarksModel";
import { PageDownloader } from "../bookmarks/PageDownloader";
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

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);
const body = "Some words worth reading. ".repeat(40);
const PAGE = `<html><head><title>A Post</title></head><body><article><h1>A Post</h1><p>${body}</p><img src="/pic.png"></article></body></html>`;
const pages = new Map<string, { type: string; body: string | Uint8Array }>([
  ["https://example.com/post", { type: "text/html; charset=utf-8", body: PAGE }],
  ["https://example.com/pic.png", { type: "image/png", body: PNG }],
]);
const script = async (request: string) => {
  const address = new URLSearchParams(request.split("?")[1]).get("url") ?? "";
  const page = pages.get(address);
  return page
    ? new Response(page.body as BodyInit, { headers: { "Content-Type": page.type } })
    : new Response("refused", { status: 403 });
};

interface Device {
  storage: MemoryStorage;
  cloud: CloudBacking;
  /** The Bookmarks view's model, wired to Sync as the app wires it. */
  bookmarks: BookmarksModel;
  changed: number;
}

/** A Device, Connected, with the Bookmarks model as the app builds it. */
async function device(tabs?: MemoryTabGroup): Promise<Device> {
  const storage = new MemoryStorage();
  const cloud = await CloudBacking.open(
    core,
    storage,
    server.cachingFetch(),
    new DrawingStore(storage, core),
    tabs?.tab(),
  );
  const files = await BookmarkFiles.open(storage);
  const bookmarks = new BookmarksModel(files, new PageDownloader(script), () => []);
  files.onSaved = () => cloud.bookmarksSaved();
  const device = { storage, cloud, bookmarks, changed: 0 };
  cloud.onBookmarksChanged(() => {
    device.changed++;
    void bookmarks.syncDidChange();
  });
  if (tabs) return device;
  await cloud.connect(url, "user", "secret");
  await advance(0);
  return device;
}

/** Saves the example page with its Article and image on the Device, as the Save dialog does. */
async function savePost(device: Device): Promise<string> {
  const prepared = await device.bookmarks.prepare("https://example.com/post");
  if (prepared.kind !== "draft") throw new Error(prepared.kind);
  const saved = await device.bookmarks.save(prepared.draft, "", ["read"], true);
  await device.bookmarks.saving;
  return saved.id;
}

/** A Sync step now, as when the tab comes back to the front. */
async function sync(device: Device): Promise<void> {
  device.cloud.setVisible(false);
  device.cloud.setVisible(true);
  await advance(0);
  await flush();
}

const settle = () => advance(10_000);

describe("the Bookmarks folder", () => {
  it("Syncs a Bookmark with its image and Markup to another Device, and back", async () => {
    const browser = await device();
    const tablet = await device();
    const id = await savePost(browser);
    const start = browser.bookmarks.article(id)!.indexOf("worth");
    browser.bookmarks.addHighlight(id, start, start + 5, "worth", 0xffeb3c);
    await browser.bookmarks.saving;
    await settle();

    const names = [...server.contents("pkm/Bookmarks").keys()];
    expect(names).toContain(`${id}.md`);
    expect(names).toContain(`${id}.json`);
    expect(names.some((name) => name.endsWith(".png"))).toBe(true);

    await sync(tablet);
    expect(tablet.changed).toBeGreaterThan(0);
    expect(tablet.bookmarks.all.map((b) => [b.title, b.hasArticle, b.tags])).toEqual([
      ["A Post", true, ["read"]],
    ]);
    const image = /([a-z0-9]+\.png)/.exec(tablet.bookmarks.article(id)!)![1]!;
    expect(await tablet.storage.read(`Bookmarks/${image}`)).toEqual(PNG);
    expect(tablet.bookmarks.markup(id).highlights.map((h) => h.quotedText)).toEqual(["worth"]);

    // And back: a rename and a second Highlight made on the tablet.
    tablet.bookmarks.rename(id, "Renamed");
    tablet.bookmarks.addHighlight(id, start + 6, start + 11, "words", 0x7ed957);
    await tablet.bookmarks.saving;
    await settle();
    await sync(browser);
    expect(browser.bookmarks.bookmark(id)?.title).toBe("Renamed");
    expect(browser.bookmarks.markup(id).highlights.map((h) => h.quotedText)).toEqual([
      "worth",
      "words",
    ]);
  });

  it("Deletes a Bookmark on the other Device", async () => {
    const browser = await device();
    const tablet = await device();
    const id = await savePost(browser);
    await settle();
    await sync(tablet);
    expect(tablet.bookmarks.all).toHaveLength(1);

    browser.bookmarks.delete(id);
    await browser.bookmarks.saving;
    await settle();
    await sync(tablet);
    expect(tablet.bookmarks.all).toHaveLength(0);
    expect([...server.contents("pkm/Bookmarks").keys()].filter((n) => n.endsWith(".md"))).toEqual(
      [],
    );
  });

  it("tells the other tabs when one saved Bookmark files", async () => {
    const tabs = new MemoryTabGroup();
    const one = await device(tabs);
    const two = await device(tabs);
    one.cloud.bookmarksSaved();
    expect([one.changed, two.changed]).toEqual([0, 1]);
  });
});

describe("the Highlight Colors", () => {
  it("reach a Bookmark's palette from another Device", async () => {
    const browser = await device();
    const tablet = await device();
    await browser.storage.write(
      "Colors/colors.json",
      bytes(JSON.stringify({ custom_colors: { "0000000000001-a": 32896 }, default_color: null })),
    );
    await settle();
    await sync(tablet);

    expect(text(await tablet.storage.read("Colors/colors.json"))).toContain("0000000000001-a");
    const palette = await tablet.bookmarks.highlightColors();
    expect(palette.colors.map((c) => c.rgb)).toContain(32896);
  });
});
