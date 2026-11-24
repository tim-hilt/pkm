import { describe, expect, it } from "vitest";
import type { Dimensions, Viewport } from "../../core/Core";
import {
  type DimensionLines,
  type DimensionNumbers,
  DimensionsDisplay,
  type DimensionsEditor,
  type FadeClock,
} from "./DimensionsDisplay";
import type { GeometrySource } from "./GeometryBuffers";

const output = (text: string) => {
  const geometry = {
    freed: false,
    free() {
      this.freed = true;
    },
  };
  const dimensions = {
    geometry,
    labels: [{ text, x: 0, y: 0, angle: 0 }],
    color: 0,
    opacity: 1,
  } as unknown as Dimensions;
  return { dimensions, geometry };
};

class Rig implements DimensionsEditor, DimensionLines, DimensionNumbers, FadeClock {
  setting = true;
  revision = 1;
  current: Dimensions | undefined;
  time = 0;
  frames: (() => void)[] = [];
  linesShown: { lines: GeometrySource | undefined; strength: number } | undefined;
  numbersShown: { text: string | undefined; strength: number } | undefined;
  readonly display = new DimensionsDisplay(this, this, this, this);

  showDimensions = () => this.setting;
  overlayRevision = () => this.revision;
  dimensions = () => this.current;
  viewport = (): Viewport => ({ originX: 0, originY: 0, zoom: 1 });
  setDimensions(lines: GeometrySource | undefined, strength: number) {
    this.linesShown = { lines, strength };
  }
  show(dimensions: Dimensions | undefined, _viewport: Viewport, strength: number) {
    this.numbersShown = { text: dimensions?.labels[0].text, strength };
  }
  now = () => this.time;
  requestFrame(callback: () => void) {
    this.frames.push(callback);
    return this.frames.length;
  }
  cancelFrame() {
    this.frames = [];
  }

  /** The core's output changes to `next`, with the overlay revision. */
  coreShows(next: Dimensions | undefined) {
    this.current = next;
    this.revision++;
    this.display.sync();
  }
  /** Runs the frame that was asked for, `ms` later. */
  frameAfter(ms: number) {
    this.time += ms;
    const [frame] = this.frames.splice(0);
    frame?.();
  }
}

describe("the Dimensions display", () => {
  it("shows the core's output at full strength and asks for no frames", () => {
    const rig = new Rig();
    rig.coreShows(output("11.40").dimensions);
    expect(rig.numbersShown).toEqual({ text: "11.40", strength: 1 });
    expect(rig.linesShown?.strength).toBe(1);
    expect(rig.frames).toHaveLength(0);
  });

  it("fades the last output out over 0.4 s, then frees it", () => {
    const rig = new Rig();
    const a = output("11.40");
    rig.coreShows(a.dimensions);
    rig.time = 1000;
    rig.coreShows(undefined);
    expect(rig.numbersShown?.strength).toBe(1);
    rig.frameAfter(100);
    expect(rig.numbersShown?.strength).toBeCloseTo(0.75);
    expect(a.geometry.freed).toBe(false);
    rig.frameAfter(100);
    expect(rig.numbersShown?.strength).toBeCloseTo(0.5);
    rig.frameAfter(200);
    expect(rig.numbersShown).toEqual({ text: undefined, strength: 1 });
    expect(rig.linesShown?.lines).toBeUndefined();
    expect(a.geometry.freed).toBe(true);
    expect(rig.frames).toHaveLength(0);
  });

  it("goes back to full strength when a new output appears during the fade", () => {
    const rig = new Rig();
    const a = output("a");
    const b = output("b");
    rig.coreShows(a.dimensions);
    rig.coreShows(undefined);
    rig.frameAfter(200);
    rig.coreShows(b.dimensions);
    expect(rig.numbersShown).toEqual({ text: "b", strength: 1 });
    expect(a.geometry.freed).toBe(true);
    expect(b.geometry.freed).toBe(false);
  });

  it("reads the output only when the overlay moved", () => {
    const rig = new Rig();
    rig.coreShows(output("a").dimensions);
    rig.current = undefined;
    rig.display.sync();
    expect(rig.numbersShown?.text).toBe("a");
  });

  it("shows nothing, and does not fade, while the setting is off", () => {
    const rig = new Rig();
    const a = output("a");
    rig.coreShows(a.dimensions);
    rig.setting = false;
    rig.display.sync();
    expect(rig.numbersShown?.text).toBeUndefined();
    expect(rig.frames).toHaveLength(0);
    expect(a.geometry.freed).toBe(true);
  });

  it("frees what is shown on dispose", () => {
    const rig = new Rig();
    const a = output("a");
    rig.coreShows(a.dimensions);
    rig.display.dispose();
    expect(a.geometry.freed).toBe(true);
  });
});
