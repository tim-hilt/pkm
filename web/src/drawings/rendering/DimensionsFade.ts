/**
 * What of the Dimensions is on screen when, a port of the Swift
 * `DimensionsFade`: the core's output goes away at once, the client lets the
 * last one fade out (the core has no timers). Pure state over a time in
 * milliseconds that the caller passes in.
 */
export class DimensionsFade<T> {
  /** Milliseconds from the end of a resize until the Dimensions are gone. */
  static readonly duration = 400;

  private last: T | undefined;
  /** When the output went away; undefined while it is there. */
  private fadeStart: number | undefined;

  /**
   * Takes the core's current output at `now`. A new output replaces a fading
   * one at full strength; none starts or continues the fade. Nothing fades in.
   */
  update(output: T | undefined, now: number): void {
    if (output !== undefined) {
      this.last = output;
      this.fadeStart = undefined;
    } else if (this.last !== undefined && this.fadeStart === undefined) {
      this.fadeStart = now;
    }
  }

  /** Forgets everything, without fading: for a setting switched off. */
  reset(): void {
    this.last = this.fadeStart = undefined;
  }

  /** The Dimensions to draw at `now` and their strength, 1 for full down to just above 0; undefined once they have faded out. */
  visible(now: number): { dimensions: T; strength: number } | undefined {
    if (this.last === undefined) return undefined;
    if (this.fadeStart === undefined) return { dimensions: this.last, strength: 1 };
    const left = 1 - (now - this.fadeStart) / DimensionsFade.duration;
    return left > 0 ? { dimensions: this.last, strength: left } : undefined;
  }

  /** Whether the picture changes with time at `now`. */
  isFading(now: number): boolean {
    return this.fadeStart !== undefined && this.visible(now) !== undefined;
  }
}
