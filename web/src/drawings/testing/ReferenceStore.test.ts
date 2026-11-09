import { describe, expect, it } from "vitest";
import { type RenderedImage, bytesPerPixel } from "../rendering/RenderedImage";
import { type PngCodec, type ReferenceFiles, ReferenceStore } from "./ReferenceStore";

/**
 * The store's contract with a developer: what happens on first run, on a
 * match, on a regression and when deliberately re-recording.
 */

/** "PNG" files that hold the raw pixels after a size header: enough for the store. */
const codec: PngCodec = {
  async encode(image) {
    return Uint8Array.from([image.width, image.height, ...image.rgba]);
  },
  async decode(bytes) {
    const [width, height, ...rgba] = bytes;
    return bytes.length > 2 ? { width, height, rgba: Uint8Array.from(rgba) } : undefined;
  },
};

class MemoryFiles implements ReferenceFiles {
  readonly files = new Map<string, Uint8Array>();
  async read(path: string) {
    return this.files.get(path);
  }
  async write(path: string, bytes: Uint8Array) {
    this.files.set(path, bytes);
  }
  async remove(path: string) {
    this.files.delete(path);
  }
}

function image(grey: number): RenderedImage {
  return { width: 20, height: 20, rgba: new Uint8Array(20 * 20 * bytesPerPixel).fill(grey) };
}

function setup() {
  const files = new MemoryFiles();
  const store = (recording = false) => new ReferenceStore(files, codec, recording);
  return { files, store };
}

describe("ReferenceStore", () => {
  it("fails a missing reference and says how to record it", async () => {
    const { files, store } = setup();

    const outcome = await store().check(image(255), "new");

    expect(outcome.status).toBe("failed");
    expect(outcome.status === "failed" && outcome.message).toContain("make web-record-references");
    expect(files.files.has("references/new.png")).toBe(false);
  });

  it("records a reference that later runs match", async () => {
    const { files, store } = setup();

    expect(await store(true).check(image(255), "new")).toEqual({ status: "recorded" });
    expect(files.files.has("references/new.png")).toBe(true);
    expect(await store().check(image(255), "new")).toEqual({ status: "matched" });
  });

  it("leaves a matching reference untouched when recording", async () => {
    const { files, store } = setup();
    await store(true).check(image(100), "stable");
    const before = files.files.get("references/stable.png");

    // Within tolerance: would be churn in version control if rewritten.
    expect(await store(true).check(image(105), "stable")).toEqual({ status: "matched" });
    expect(files.files.get("references/stable.png")).toBe(before);
  });

  it("replaces a changed reference when recording", async () => {
    const { store } = setup();
    await store(true).check(image(0), "changed");

    expect(await store(true).check(image(255), "changed")).toEqual({ status: "recorded" });
    expect(await store().check(image(255), "changed")).toEqual({ status: "matched" });
  });

  it("fails a regression and leaves the actual and diff images behind", async () => {
    const { files, store } = setup();
    await store(true).check(image(0), "regress");

    const outcome = await store().check(image(255), "regress");

    expect(outcome.status).toBe("failed");
    expect(outcome.status === "failed" && outcome.message).toContain("regress-diff.png");
    expect(await codec.decode(files.files.get("reference-failures/regress-actual.png")!)).toEqual(
      image(255),
    );
    expect(files.files.has("reference-failures/regress-expected.png")).toBe(true);
    expect(files.files.has("reference-failures/regress-diff.png")).toBe(true);
  });

  it("removes stale failure images once the check passes", async () => {
    const { files, store } = setup();
    await store(true).check(image(0), "healed");
    await store().check(image(255), "healed");

    expect(await store().check(image(0), "healed")).toEqual({ status: "matched" });
    for (const kind of ["actual", "expected", "diff"]) {
      expect(files.files.has(`reference-failures/healed-${kind}.png`), kind).toBe(false);
    }
  });
});
