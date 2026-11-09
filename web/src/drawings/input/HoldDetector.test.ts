import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HoldDetector } from "./HoldDetector";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function detector() {
  const holds: [number, number][] = [];
  return { holds, hold: new HoldDetector((x, y) => holds.push([x, y]), 500, 10) };
}

describe("a hold", () => {
  it("is a finger kept down inside the slop for the duration, reported where it went down", () => {
    const { holds, hold } = detector();
    hold.down(1, 100, 100);
    hold.move(1, 104, 103);
    vi.advanceTimersByTime(499);
    expect(holds).toEqual([]);
    vi.advanceTimersByTime(1);
    expect(holds).toEqual([[100, 100]]);
  });

  it("is none when the finger lifts first", () => {
    const { holds, hold } = detector();
    hold.down(1, 100, 100);
    hold.up(1);
    vi.advanceTimersByTime(1000);
    expect(holds).toEqual([]);
  });

  it("is none once the finger moved beyond the slop: that is a drag", () => {
    const { holds, hold } = detector();
    hold.down(1, 100, 100);
    hold.move(1, 120, 100);
    vi.advanceTimersByTime(1000);
    expect(holds).toEqual([]);
  });

  it("is none with a second finger down: that is a pinch", () => {
    const { holds, hold } = detector();
    hold.down(1, 100, 100);
    hold.down(2, 200, 200);
    vi.advanceTimersByTime(1000);
    expect(holds).toEqual([]);
  });

  it("is reported once", () => {
    const { holds, hold } = detector();
    hold.down(1, 100, 100);
    vi.advanceTimersByTime(2000);
    expect(holds).toHaveLength(1);
  });
});
