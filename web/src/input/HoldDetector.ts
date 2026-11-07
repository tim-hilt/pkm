/**
 * Detects the hold of a finger that opens the Context menu: kept down inside
 * a small slop for a while. Moving first is a drag, a second finger a pinch,
 * lifting a tap; none of them is a hold. The client only detects it: the core
 * decides what it means (ADR-0006).
 */
export class HoldDetector {
  private id: number | undefined;
  private x = 0;
  private y = 0;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private fingers = 0;

  constructor(
    private readonly onHold: (x: number, y: number) => void,
    private readonly duration = 500,
    private readonly slop = 10,
  ) {}

  down(id: number, x: number, y: number): void {
    this.fingers++;
    if (this.fingers > 1) {
      this.cancel();
      return;
    }
    this.id = id;
    this.x = x;
    this.y = y;
    this.timer = setTimeout(() => {
      this.timer = undefined;
      this.onHold(this.x, this.y);
    }, this.duration);
  }

  move(id: number, x: number, y: number): void {
    if (id === this.id && Math.hypot(x - this.x, y - this.y) > this.slop) this.cancel();
  }

  up(id: number): void {
    this.fingers = Math.max(0, this.fingers - 1);
    if (id === this.id) this.cancel();
  }

  cancel(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    this.id = undefined;
  }
}
