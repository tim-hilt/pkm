import type { Dimensions, Viewport } from "../../core/Core";
import type { GeometrySource } from "./GeometryBuffers";
import { DimensionsFade } from "./DimensionsFade";

/** What the editor offers a Dimensions display; `Editor` satisfies it. */
export interface DimensionsEditor {
  showDimensions(): boolean;
  overlayRevision(): number;
  dimensions(): Dimensions | undefined;
  viewport(): Viewport;
}

/** What draws the lines of the Dimensions; `CanvasRenderer` satisfies it. */
export interface DimensionLines {
  setDimensions(lines: GeometrySource | undefined, strength: number): void;
}

/** What draws the numbers of the Dimensions; `DimensionLabels` satisfies it. */
export interface DimensionNumbers {
  /** Shows `dimensions`' numbers in `viewport` at `strength` (1 full, down to 0); undefined takes them away. */
  show(dimensions: Dimensions | undefined, viewport: Viewport, strength: number): void;
}

/** Time and frames, so that a test can run the fade by hand. */
export interface FadeClock {
  /** Milliseconds, steadily rising. */
  now(): number;
  requestFrame(callback: () => void): number;
  cancelFrame(handle: number): void;
}

const browserClock: FadeClock = {
  now: () => performance.now(),
  requestFrame: (callback) => requestAnimationFrame(callback),
  cancelFrame: (handle) => cancelAnimationFrame(handle),
};

/**
 * Shows the editor's Dimensions over the Canvas (the Swift `CanvasView`'s
 * `updateDimensions`): lines through the renderer, numbers through the
 * system-font labels. When the core's output goes away the last one fades out
 * over `DimensionsFade.duration`; one that appears during the fade replaces
 * it at full strength. Frames are asked for only while a fade runs.
 */
export class DimensionsDisplay {
  private readonly fade = new DimensionsFade<Dimensions>();
  /** What the lines and numbers were last given; owns the geometry to free. */
  private shown: Dimensions | undefined;
  private readValue: number | undefined;
  private frame: number | undefined;

  constructor(
    private readonly editor: DimensionsEditor,
    private readonly lines: DimensionLines,
    private readonly numbers: DimensionNumbers,
    private readonly clock: FadeClock = browserClock,
  ) {}

  /** Call after input or a command reached the editor: reads the core's output when its overlay moved, and shows what is due. */
  sync(): void {
    const now = this.clock.now();
    if (!this.editor.showDimensions()) {
      // Nothing fades while the setting is off.
      this.fade.reset();
      this.readValue = undefined;
    } else if (this.editor.overlayRevision() !== this.readValue) {
      this.readValue = this.editor.overlayRevision();
      this.fade.update(this.editor.dimensions(), now);
    }
    this.draw(now);
  }

  /** Stops fading and frees what is shown; call once the lines and numbers are no longer drawn. */
  dispose(): void {
    if (this.frame !== undefined) this.clock.cancelFrame(this.frame);
    this.frame = undefined;
    this.fade.reset();
    this.shown?.geometry.free();
    this.shown = undefined;
  }

  private draw(now: number): void {
    const visible = this.fade.visible(now);
    const strength = visible?.strength ?? 1;
    this.lines.setDimensions(visible?.dimensions.geometry, strength);
    this.numbers.show(visible?.dimensions, this.editor.viewport(), strength);
    if (this.shown !== visible?.dimensions) {
      this.shown?.geometry.free();
      this.shown = visible?.dimensions;
    }
    if (this.fade.isFading(now) && this.frame === undefined)
      this.frame = this.clock.requestFrame(() => this.tick());
  }

  private tick(): void {
    this.frame = undefined;
    this.draw(this.clock.now());
  }
}
