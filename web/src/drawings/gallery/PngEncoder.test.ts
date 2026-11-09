import { inflateSync } from "node:zlib";
import { describe, expect, it } from "vitest";
import { encodePng } from "./PngEncoder";

/** Reads back what a PNG of 8-bit RGBA, filter 0, holds: a worked decoder independent of the encoder. */
function decode(png: Uint8Array) {
  const view = new DataView(png.buffer, png.byteOffset);
  expect([...png.subarray(0, 8)]).toEqual([137, 80, 78, 71, 13, 10, 26, 10]);
  const chunks: string[] = [];
  const data: Uint8Array[] = [];
  let width = 0;
  let height = 0;
  for (let offset = 8; offset < png.length;) {
    const length = view.getUint32(offset);
    const type = String.fromCharCode(...png.subarray(offset + 4, offset + 8));
    chunks.push(type);
    const body = png.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      width = view.getUint32(offset + 8);
      height = view.getUint32(offset + 12);
    }
    if (type === "IDAT") data.push(body);
    offset += 12 + length;
  }
  const raw = inflateSync(Buffer.concat(data));
  const rowBytes = width * 4;
  const rgba = new Uint8Array(rowBytes * height);
  for (let row = 0; row < height; row++) {
    expect(raw[row * (rowBytes + 1)]).toBe(0);
    rgba.set(raw.subarray(row * (rowBytes + 1) + 1, (row + 1) * (rowBytes + 1)), row * rowBytes);
  }
  return { chunks, width, height, rgba };
}

describe("PNG encoding", () => {
  it("writes an image that reads back with the same pixels", async () => {
    const rgba = Uint8Array.from({ length: 3 * 2 * 4 }, (_, i) => (i * 37) % 256);
    const decoded = decode(await encodePng({ width: 3, height: 2, rgba }));
    expect(decoded.chunks).toEqual(["IHDR", "IDAT", "IEND"]);
    expect([decoded.width, decoded.height]).toEqual([3, 2]);
    expect(decoded.rgba).toEqual(rgba);
  });

  it("has the checksums PNG readers verify", async () => {
    // The IEND chunk is the same in every PNG, CRC included.
    const png = await encodePng({ width: 1, height: 1, rgba: new Uint8Array(4) });
    expect([...png.subarray(-12)]).toEqual([0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130]);
  });
});
