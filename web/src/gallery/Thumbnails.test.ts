import { beforeEach, describe, expect, it } from "vitest";
import type { RenderedImage } from "../rendering/RenderedImage";
import { MemoryStorage } from "../storage/MemoryStorage";
import { DrawingStore, type Entry } from "./DrawingStore";
import { type ThumbnailPainter, Thumbnails } from "./Thumbnails";

const bytes = (text: string) => new TextEncoder().encode(text);

/** Paints a one-pixel image: white or dark paper; `ink` Drawings get a black pixel. A Drawing that is not "{}" or "ink" is no Drawing. */
class FakePainter implements ThumbnailPainter {
  async render(drawing: Uint8Array, dark: boolean): Promise<RenderedImage | undefined> {
    const text = new TextDecoder().decode(drawing);
    if (text !== "{}" && text !== "ink") return undefined;
    const paper = dark ? 30 : 255;
    const pixel = text === "ink" ? 0 : paper;
    return {
      width: 2,
      height: 1,
      rgba: Uint8Array.of(paper, paper, paper, 255, pixel, pixel, pixel, 255),
    };
  }
}

let store: DrawingStore;
let thumbnails: Thumbnails;
beforeEach(() => {
  store = new DrawingStore(new MemoryStorage());
  thumbnails = new Thumbnails(store, new FakePainter());
});

const entry = async (name: string): Promise<Entry> =>
  (await store.list()).find((e) => e.name === name)!;

/** The PNG's first pixel is stored in the pixel data; reading a PNG back is the browser's job, so the tests tell pictures apart by size. */
const imageOf = async (listed: Entry, dark = false) => {
  const thumbnail = await thumbnails.thumbnail(listed, dark);
  if (thumbnail.kind !== "image") throw new Error(`no image, but ${thumbnail.kind}`);
  return thumbnail.png;
};

describe("thumbnails", () => {
  it("are drawn for a Drawing and cached outside the Drawing's folder", async () => {
    await store.save(bytes("ink"), "a");
    const before = await entry("a");

    const png = await imageOf(before);

    expect([...png.subarray(1, 4)]).toEqual([80, 78, 71]); // "PNG"
    expect(thumbnails.rendered).toBe(1);
    expect(await store.thumbnail(before)).toEqual(png);
    expect(await store.open("a")).toEqual(bytes("ink"));
    expect(await entry("a")).toEqual(before);
  });

  it("are not drawn again when asked again", async () => {
    await store.save(bytes("ink"), "a");
    const first = await imageOf(await entry("a"));
    const second = await imageOf(await entry("a"));
    expect(second).toEqual(first);
    expect(thumbnails.rendered).toBe(1);
  });

  it("are not drawn twice when asked for twice at once", async () => {
    await store.save(bytes("ink"), "a");
    const listed = await entry("a");
    await Promise.all([thumbnails.thumbnail(listed), thumbnails.thumbnail(listed)]);
    expect(thumbnails.rendered).toBe(1);
  });

  it("come from the cache after a restart, without drawing", async () => {
    await store.save(bytes("ink"), "a");
    await imageOf(await entry("a"));

    const later = new Thumbnails(store, new FakePainter());
    expect((await later.thumbnail(await entry("a"))).kind).toBe("image");
    expect(later.rendered).toBe(0);
  });

  it("are made afresh for a changed Drawing", async () => {
    await store.save(bytes("{}"), "a");
    const blank = await imageOf(await entry("a"));
    await store.save(bytes("ink"), "a");
    const changed = await imageOf(await entry("a"));
    expect(changed).not.toEqual(blank);
    expect(thumbnails.rendered).toBe(2);
  });

  it("are kept per appearance, each made when first needed and cached once", async () => {
    await store.save(bytes("{}"), "a");
    const listed = await entry("a");
    const dark = await imageOf(listed, true);
    expect(await store.thumbnail(listed)).toBeUndefined();
    const light = await imageOf(listed);
    expect(light).not.toEqual(dark);
    expect(thumbnails.rendered).toBe(2);

    const later = new Thumbnails(store, new FakePainter());
    await later.thumbnail(listed, true);
    await later.thumbnail(listed);
    expect(later.rendered).toBe(0);
  });

  it("of a changed Drawing replace both appearances", async () => {
    await store.save(bytes("{}"), "a");
    await imageOf(await entry("a"));
    await imageOf(await entry("a"), true);
    await store.save(bytes("ink"), "a");
    const changed = await entry("a");
    expect(await store.thumbnail(changed)).toBeUndefined();
    expect(await store.thumbnail(changed, true)).toBeUndefined();
    await imageOf(changed);
    await imageOf(changed, true);
    expect(thumbnails.rendered).toBe(4);
  });

  it("of a file that is no Drawing say so, also when asked again, and touch nothing", async () => {
    await store.save(bytes('{"elements": ['), "broken");
    const broken = await entry("broken");

    expect((await thumbnails.thumbnail(broken)).kind).toBe("unreadable");
    expect((await thumbnails.thumbnail(broken)).kind).toBe("unreadable");

    expect(await store.open("broken")).toEqual(bytes('{"elements": ['));
    expect(await entry("broken")).toEqual(broken);
    expect(await store.thumbnail(broken)).toBeUndefined();
    expect(thumbnails.rendered).toBe(0);
  });

  it("of a file that was fixed are no longer unreadable", async () => {
    await store.save(bytes("not json"), "a");
    expect((await thumbnails.thumbnail(await entry("a"))).kind).toBe("unreadable");
    await store.save(bytes("ink"), "a");
    expect((await thumbnails.thumbnail(await entry("a"))).kind).toBe("image");
  });

  it("are unavailable for a Drawing that is gone", async () => {
    await store.save(bytes("ink"), "gone");
    const listed = await entry("gone");
    await store.delete("gone");
    expect((await thumbnails.thumbnail(listed)).kind).toBe("unavailable");
  });

  it("are not drawn for many Drawings that have cached ones", async () => {
    const png = await imageOf((await store.save(bytes("ink"), "seed"), await entry("seed")));
    for (let number = 0; number < 150; number++) {
      await store.save(bytes("{}"), `drawing ${number}`);
      await store.saveThumbnail(png, await entry(`drawing ${number}`));
    }
    const fresh = new Thumbnails(store, new FakePainter());
    for (const listed of await store.list()) {
      if (listed.name !== "seed") await fresh.thumbnail(listed);
    }
    expect(fresh.rendered).toBe(0);
  });
});
