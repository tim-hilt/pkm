import { describe, expect, it, vi } from "vitest";
import { type ImageFileSource, ImageTextures } from "./ImageTextures";

/** A stand-in for a texture that notices being released. */
function fakeTexture() {
  return { destroy: vi.fn() } as unknown as GPUTexture & { destroy: ReturnType<typeof vi.fn> };
}

/** Files by id, counting how often they are asked for. */
class FakeFiles implements ImageFileSource {
  requests: string[] = [];
  constructor(public files: Record<string, Uint8Array> = {}) {}
  fileBytes(id: string) {
    this.requests.push(id);
    return this.files[id];
  }
}

const picture = new Uint8Array([1, 2, 3]);
const junk = new Uint8Array([0]);

/** Textures whose loader makes a texture of every file except the one whose bytes are `junk`. */
function makeTextures() {
  const made: ReturnType<typeof fakeTexture>[] = [];
  const textures = new ImageTextures(async (bytes) => {
    if (bytes === junk) return undefined;
    made.push(fakeTexture());
    return made[made.length - 1];
  });
  return { textures, made };
}

describe("ImageTextures", () => {
  it("has no texture while a file loads, then has the texture and says so", async () => {
    const { textures, made } = makeTextures();
    const files = new FakeFiles({ a: picture });
    const changed: string[] = [];
    textures.onChange = (id) => changed.push(id);

    expect(textures.texture("a", files)).toBeUndefined();
    expect(changed).toEqual([]);
    await textures.settled();

    expect(textures.texture("a", files)).toBe(made[0]);
    expect(changed).toEqual(["a"]);
  });

  it("reads and decodes a file once however often it is asked for", async () => {
    const { textures } = makeTextures();
    const files = new FakeFiles({ a: picture });

    textures.texture("a", files);
    textures.texture("a", files);
    await textures.settled();
    const first = textures.texture("a", files);
    const second = textures.texture("a", files);

    expect(first).toBeDefined();
    expect(first).toBe(second);
    expect(files.requests).toEqual(["a"]);
    expect(textures.decodeCount).toBe(1);
  });

  it("does not read the file while the caller may still look at the core's buffers", () => {
    const { textures } = makeTextures();
    const files = new FakeFiles({ a: picture });

    textures.texture("a", files);

    expect(files.requests).toEqual([]);
  });

  it("has nothing for a missing file, which is not asked for again and is not broken", async () => {
    const { textures } = makeTextures();
    const files = new FakeFiles();
    const changed = vi.fn();
    textures.onChange = changed;

    textures.texture("gone", files);
    await textures.settled();
    expect(textures.texture("gone", files)).toBeUndefined();
    expect(textures.texture("none", undefined)).toBeUndefined();
    await textures.settled();

    expect(files.requests).toEqual(["gone"]);
    expect(textures.isUndecodable("gone")).toBe(false);
    expect(textures.count).toBe(0);
    expect(changed).not.toHaveBeenCalled();
  });

  it("remembers a file that is no image as undecodable", async () => {
    const { textures } = makeTextures();
    const files = new FakeFiles({ junk });
    const changed: string[] = [];
    textures.onChange = (id) => changed.push(id);

    textures.texture("junk", files);
    await textures.settled();
    textures.texture("junk", files);

    expect(textures.isUndecodable("junk")).toBe(true);
    expect(textures.decodeCount).toBe(1);
    expect(changed).toEqual(["junk"]);
  });

  it("releases the textures of files nothing refers to any more", async () => {
    const { textures, made } = makeTextures();
    const files = new FakeFiles({ a: picture, b: picture });
    textures.texture("a", files);
    textures.texture("b", files);
    await textures.settled();
    expect(textures.count).toBe(2);

    textures.keepOnly(new Set(["a"]));

    expect(textures.count).toBe(1);
    expect(made[0].destroy).not.toHaveBeenCalled();
    expect(made[1].destroy).toHaveBeenCalled();
    expect(textures.texture("a", files)).toBe(made[0]);
    expect(files.requests).toEqual(["a", "b"]);
  });

  it("throws away a texture that arrives for a file released while it loaded", async () => {
    let arrive: (texture: GPUTexture) => void = () => {};
    const textures = new ImageTextures(() => new Promise((resolve) => (arrive = resolve)));
    const changed = vi.fn();
    textures.onChange = changed;
    textures.texture("a", new FakeFiles({ a: picture }));
    await vi.waitFor(() => expect(textures.decodeCount).toBe(1));

    textures.keepOnly(new Set());
    const late = fakeTexture();
    arrive(late);
    await textures.settled();

    expect(textures.count).toBe(0);
    expect(late.destroy).toHaveBeenCalled();
    expect(changed).not.toHaveBeenCalled();
  });

  it("lets a file that failed be tried again after forgetting failures", async () => {
    const { textures } = makeTextures();
    const files = new FakeFiles();
    textures.texture("late", files);
    await textures.settled();

    files.files.late = picture;
    textures.forgetFailures();
    textures.texture("late", files);
    await textures.settled();

    expect(textures.texture("late", files)).toBeDefined();
  });
});
