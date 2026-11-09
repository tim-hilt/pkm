import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { layoutFor } from "../chrome/DrawingLayout";
import { phoneQuery } from "./ShellLayout";

describe("the phone's media query", () => {
  it("is the one the page's stylesheet uses", () => {
    const page = readFileSync(new URL("../../index.html", import.meta.url), "utf8");
    expect(page).toContain(`@media ${phoneQuery} {`);
  });

  it("is the same rule as layoutFor", () => {
    // Evaluates the query's two clauses at the layout's boundary sizes.
    const matches = (width: number, height: number) =>
      phoneQuery.split(", ").some((clause) => {
        const limit = (name: string) =>
          Number(new RegExp(`\\(${name}: (\\d+)px\\)`).exec(clause)?.[1] ?? Infinity);
        const minmax = (name: string, size: number) =>
          size <= limit(`max-${name}`) &&
          size >= Number(new RegExp(`\\(min-${name}: (\\d+)px\\)`).exec(clause)?.[1] ?? 0);
        return minmax("width", width) && minmax("height", height);
      });
    for (const width of [598, 599, 600, 999, 1000])
      for (const height of [499, 500, 501])
        expect(matches(width, height), `${width}x${height}`).toBe(
          layoutFor(width, height) === "mobile",
        );
  });
});
