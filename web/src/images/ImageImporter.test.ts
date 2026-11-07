import { describe, expect, it } from "vitest";
import {
  type ImageCodec,
  type OutputImageType,
  fileIdFor,
  maxImportFileSize,
  maxImportPixelSize,
  prepareImage,
  sniffWebImageType,
} from "./ImageImporter";

const png = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);
const jpeg = Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 1, 2, 3]);
const bmp = Uint8Array.from([0x42, 0x4d, 1, 2, 3]);

/**
 * A codec over made-up pictures: the bytes stand for a picture of `size`; the
 * encoded size of a picture is `bytesPerPixel` times its pixels (less for a
 * lower JPEG quality), so what is too large and what fits is up to the test.
 */
function fakeCodec(
  size: { width: number; height: number } | undefined,
  { bytesPerPixel = 0.001, jpegQualityFactor = 1 } = {},
) {
  const encodes: { maxSide: number; type: OutputImageType; quality: number }[] = [];
  const codec: ImageCodec = {
    async size() {
      return size;
    },
    async encode(_bytes, maxSide, type, quality) {
      encodes.push({ maxSide, type, quality });
      const factor = Math.min(1, maxSide / Math.max(size!.width, size!.height));
      const width = Math.round(size!.width * factor);
      const height = Math.round(size!.height * factor);
      const length = Math.ceil(
        width * height * bytesPerPixel * (type === "image/jpeg" ? quality * jpegQualityFactor : 1),
      );
      return { bytes: new Uint8Array(length), width, height };
    },
  };
  return { codec, encodes };
}

describe("sniffWebImageType", () => {
  it("knows the formats every browser reads by their magic numbers", () => {
    expect(sniffWebImageType(png)).toBe("image/png");
    expect(sniffWebImageType(jpeg)).toBe("image/jpeg");
    expect(sniffWebImageType(new TextEncoder().encode("GIF89a"))).toBe("image/gif");
    expect(sniffWebImageType(new TextEncoder().encode("RIFF\0\0\0\0WEBPVP8 "))).toBe("image/webp");
  });

  it("does not know any other, such as BMP, or a RIFF that is not WebP", () => {
    expect(sniffWebImageType(bmp)).toBeUndefined();
    expect(sniffWebImageType(new TextEncoder().encode("RIFF\0\0\0\0WAVEfmt "))).toBeUndefined();
    expect(sniffWebImageType(new Uint8Array())).toBeUndefined();
  });
});

describe("fileIdFor", () => {
  it("is the SHA-1 of the bytes as lower-case hex", async () => {
    // The test vector of FIPS 180: SHA-1("abc").
    expect(await fileIdFor(new TextEncoder().encode("abc"))).toBe(
      "a9993e364706816aba3e25717850c26c9cd0d89d",
    );
  });
});

describe("prepareImage", () => {
  it("keeps an image within the limits exactly as it is", async () => {
    const { codec, encodes } = fakeCodec({ width: 40, height: 40 });

    const file = await prepareImage(png, codec);

    expect(file?.bytes).toBe(png);
    expect(file?.mimeType).toBe("image/png");
    expect([file?.width, file?.height]).toEqual([40, 40]);
    expect(encodes).toEqual([]);
  });

  it("downscales a wide image to the limit on its long side, keeping its proportions", async () => {
    const { codec } = fakeCodec({ width: 3000, height: 1000 });

    const file = await prepareImage(png, codec);

    expect([file?.width, file?.height]).toEqual([1440, 480]);
    expect(file?.mimeType).toBe("image/png");
    expect(file?.fileId).toBe(await fileIdFor(png));
  });

  it("downscales a tall image to the limit on its height", async () => {
    const { codec } = fakeCodec({ width: 500, height: 2880 });

    const file = await prepareImage(png, codec);

    expect([file?.width, file?.height]).toEqual([250, 1440]);
  });

  it("shrinks an image over the size limit by a quarter at a time until it fits", async () => {
    // 1600 x 1000 px at 3 bytes a pixel is 4.8 MB; 1440 x 900 is 3.9 MB: that fits.
    const { codec, encodes } = fakeCodec({ width: 1600, height: 1000 }, { bytesPerPixel: 3 });
    const big = new Uint8Array(maxImportFileSize + 1);
    big.set(png);

    const file = await prepareImage(big, codec);

    expect(file!.bytes.length).toBeLessThanOrEqual(maxImportFileSize);
    expect(Math.max(file!.width, file!.height)).toBeLessThanOrEqual(maxImportPixelSize);
    expect(encodes[0].maxSide).toBe(1440);
    expect(file!.width * 10).toBe(file!.height * 16);
  });

  it("steps down in sizes when even the first does not fit", async () => {
    const { codec, encodes } = fakeCodec({ width: 1600, height: 1000 }, { bytesPerPixel: 12 });

    await prepareImage(png, codec);

    expect(encodes.map((e) => e.maxSide).slice(0, 3)).toEqual([1440, 1080, 810]);
  });

  it("recompresses a JPEG over the size limit as JPEG, harder before it shrinks", async () => {
    // At full quality 1400 x 1400 px is 5.8 MB; at 0.6 it is 3.5 MB.
    const { codec, encodes } = fakeCodec({ width: 1400, height: 1400 }, { bytesPerPixel: 3.5 });
    const big = new Uint8Array(maxImportFileSize + 1);
    big.set(jpeg);

    const file = await prepareImage(big, codec);

    expect(file?.mimeType).toBe("image/jpeg");
    expect(encodes.map((e) => [e.maxSide, e.quality])).toEqual([
      [1400, 0.85],
      [1400, 0.6],
    ]);
    expect(file!.bytes.length).toBeLessThanOrEqual(maxImportFileSize);
    expect([file!.width, file!.height]).toEqual([1400, 1400]);
  });

  it("converts a format the web cannot read even when small, to PNG", async () => {
    const { codec } = fakeCodec({ width: 40, height: 40 });

    const file = await prepareImage(bmp, codec);

    expect(file?.mimeType).toBe("image/png");
    expect([file?.width, file?.height]).toEqual([40, 40]);
  });

  it("gives up when the picture cannot be encoded", async () => {
    const { codec } = fakeCodec({ width: 1000, height: 1000 });
    const big = new Uint8Array(maxImportFileSize + 1);
    big.set(png);

    expect(await prepareImage(big, { ...codec, encode: async () => undefined })).toBeUndefined();
  });

  it("gives up when even the smallest picture is too large", async () => {
    const { codec } = fakeCodec({ width: 1000, height: 1000 });
    const big = new Uint8Array(maxImportFileSize + 1);
    big.set(png);
    const tooLarge = { bytes: big, width: 1, height: 1 };

    expect(await prepareImage(big, { ...codec, encode: async () => tooLarge })).toBeUndefined();
  });

  it("does not import what is no image", async () => {
    const { codec } = fakeCodec(undefined);

    expect(await prepareImage(new TextEncoder().encode("not an image"), codec)).toBeUndefined();
    expect(await prepareImage(new Uint8Array(), codec)).toBeUndefined();
  });
});
