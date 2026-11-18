import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { Inline, loadCore } from "../core/Core";
import { titleSpans } from "./TitleText";

beforeAll(async () => {
  const generated = new URL("../../generated/", import.meta.url);
  await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

describe("titleSpans", () => {
  it("finds emphasis, strong and code with their marker lengths", () => {
    expect(titleSpans("*a* **b** `c`")).toEqual([
      { kind: Inline.Emphasis, start: 0, end: 3, markerLen: 1 },
      { kind: Inline.Strong, start: 4, end: 9, markerLen: 2 },
      { kind: Inline.Code, start: 10, end: 13, markerLen: 1 },
    ]);
  });

  it("counts UTF-16 units", () => {
    expect(titleSpans("😀 *a*")).toEqual([
      { kind: Inline.Emphasis, start: 3, end: 6, markerLen: 1 },
    ]);
  });

  it("leaves unmatched markers and block syntax plain", () => {
    expect(titleSpans("2 * 3")).toEqual([]);
    expect(titleSpans("# [a](b)")).toEqual([]);
  });
});
