import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";
import { redAt } from "../rendering/RenderedImage";
import { OpfsStorage } from "../storage/OpfsStorage";
import { DrawingStore } from "./DrawingStore";
import { Gallery } from "./Gallery";
import { ThumbnailRenderer, thumbnailHeight, thumbnailWidth } from "./ThumbnailRenderer";
import { Thumbnails } from "./Thumbnails";
import { Excalifont } from "../text/Excalifont";
import fontUrl from "../../generated/Excalifont-Regular.ttf?url";
import sampleUrl from "../../generated/sample.excalidraw?url";

/**
 * The Gallery's storage in the real Origin Private File System and its
 * thumbnails from the production renderer, in Chromium.
 */

async function namesIn(entries: AsyncIterable<string>): Promise<string[]> {
  const found: string[] = [];
  for await (const name of entries) found.push(name);
  return found;
}

const bytes = (text: string) => new TextEncoder().encode(text);

async function clearOpfs() {
  const root = await navigator.storage.getDirectory();
  for await (const name of root.keys()) await root.removeEntry(name, { recursive: true });
}

/** A fresh handle on the file system, as after a reload: nothing is remembered but what is stored. */
const reopened = () => new DrawingStore(OpfsStorage.open());

beforeEach(clearOpfs);

describe("Drawings on the Origin Private File System", () => {
  it("are plain .excalidraw files in a folder, with exactly the saved bytes", async () => {
    const store = reopened();
    const content = bytes('{"type":"excalidraw","version":2,"elements":[]}');

    const name = await store.add(content, "My plan");

    const root = await navigator.storage.getDirectory();
    const folder = await root.getDirectoryHandle("Drawings");
    const file = await (await folder.getFileHandle("My plan.excalidraw")).getFile();
    expect(new Uint8Array(await file.arrayBuffer())).toEqual(content);
    expect(name).toBe("My plan");
    expect(await namesIn(folder.keys())).toEqual(["My plan.excalidraw"]);
  });

  it("survive a reload, with their viewport", async () => {
    const before = reopened();
    await before.add(bytes("{}"), "kept");
    await before.saveViewport({ originX: -5, originY: 7, zoom: 2 }, "kept");

    const after = reopened();
    expect((await after.list()).map((entry) => entry.name)).toEqual(["kept"]);
    expect(await after.open("kept")).toEqual(bytes("{}"));
    expect(await after.viewport("kept")).toEqual({ originX: -5, originY: 7, zoom: 2 });
  });

  it("can be renamed, duplicated and deleted with their caches", async () => {
    const store = reopened();
    await store.add(bytes("a"), "a");
    await store.saveViewport({ originX: 1, originY: 2, zoom: 3 }, "a");

    await store.rename("a", "b");
    expect(await store.viewport("b")).toEqual({ originX: 1, originY: 2, zoom: 3 });
    expect(await store.duplicate("b")).toBe("b copy");
    await store.delete("b");

    expect((await store.list()).map((entry) => entry.name)).toEqual(["b copy"]);
    const root = await navigator.storage.getDirectory();
    const caches = await root.getDirectoryHandle("Caches");
    expect(await namesIn(caches.keys())).toEqual([]);
  });

  it("replace the earlier contents when saved again", async () => {
    const store = reopened();
    await store.save(bytes("x".repeat(5000)), "a");
    await store.save(bytes("short"), "a");
    expect(await store.open("a")).toEqual(bytes("short"));
  });
});

describe("thumbnails from the production renderer", () => {
  let harness: RenderHarness;
  let sample: Uint8Array;
  let painter: ThumbnailRenderer;
  beforeAll(async () => {
    harness = await makeRenderHarness();
    sample = new Uint8Array(await (await fetch(sampleUrl)).arrayBuffer());
    const font = new Excalifont(new Uint8Array(await (await fetch(fontUrl)).arrayBuffer()));
    painter = new ThumbnailRenderer(harness.device, harness.core, font);
  });

  it("have the size of a card, with ink on paper", async () => {
    const image = (await painter.render(sample, false))!;
    expect([image.width, image.height]).toEqual([thumbnailWidth, thumbnailHeight]);
    expect(redAt(image, 0, 0)).toBe(255);
    let ink = 0;
    for (let i = 0; i < image.width * image.height; i++) if (image.rgba[i * 4] < 200) ink++;
    expect(ink).toBeGreaterThan(500);
  });

  it("of an empty Drawing are only paper", async () => {
    const editor = harness.core.makeEditor();
    const image = (await painter.render(editor.save(), false))!;
    for (let i = 0; i < image.width * image.height; i++) expect(image.rgba[i * 4]).toBe(255);
  });

  it("show the dark Canvas in dark mode", async () => {
    const image = (await painter.render(sample, true))!;
    expect(redAt(image, 0, 0)).toBeLessThan(60);
  });

  it("are undefined for a file that is no Drawing", async () => {
    expect(await painter.render(bytes("not json"), false)).toBeUndefined();
  });

  it("are cached as PNG files beside the Drawings, which stay plain", async () => {
    const store = reopened();
    await store.add(sample, "sample");
    const thumbnails = new Thumbnails(store, painter);
    const listed = (await store.list())[0];

    const thumbnail = await thumbnails.thumbnail(listed);

    expect(thumbnail.kind).toBe("image");
    const root = await navigator.storage.getDirectory();
    expect(await namesIn((await root.getDirectoryHandle("Drawings")).keys())).toEqual([
      "sample.excalidraw",
    ]);
    // The browser decodes what was cached.
    const png = await store.thumbnail(listed);
    const decoded = await createImageBitmap(new Blob([png as Uint8Array<ArrayBuffer>]));
    expect([decoded.width, decoded.height]).toEqual([thumbnailWidth, thumbnailHeight]);
    expect(new Thumbnails(store, painter).rendered).toBe(0);
  });

  it("go through a Gallery on the file system as in the app", async () => {
    const store = reopened();
    const gallery = await Gallery.start(store, harness.core, new Thumbnails(store, painter));
    await gallery.createDrawing();
    const name = gallery.open!.name;
    gallery.open!.editor.load(sample);
    gallery.open!.editorDidChange();
    await gallery.close();

    expect(gallery.drawings.map((entry) => entry.name)).toEqual([name]);
    expect(await reopened().open(name)).toEqual(await store.open(name));
  });
});
