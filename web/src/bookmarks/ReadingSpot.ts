import { offsetAtPoint, rectAt } from "./ArticleOffsets";

/** How long the Reader rests, scrolling stopped, before its place is kept. */
const restDelay = 400;
/** Space left above a place scrolled to, in pixels. */
const margin = 12;

/**
 * Where in the Article the Reader is: Reading Progress: the UTF-16 offset of the first character at
 * the top of the screen. It reopens at the offset it is given, following the layout (Images load
 * and move the text) until the user scrolls, and keeps the offset it rests at, going back included, once
 * scrolling has stopped, when the page is left and on `flush`. Nothing is kept for an Article
 * only looked at.
 */
export class ReadingSpot {
  private target: number | undefined;
  /** Whether the user took over the scrolling: the layout is no longer followed. */
  private userScrolled = false;
  /** Whether the place differs from the one kept. */
  private moved = false;
  private timer: ReturnType<typeof setTimeout> | undefined;
  private readonly listeners: [EventTarget, string, () => void][] = [];
  private readonly resize: ResizeObserver;

  constructor(
    private readonly scroll: HTMLElement,
    private readonly article: HTMLElement,
    /** Keeps `offset` as where the Article was left. */
    private readonly keep: (offset: number) => void,
  ) {
    const touch = () => (this.userScrolled = this.moved = true);
    for (const event of ["wheel", "touchmove", "pointerdown", "keydown"]) {
      this.listen(scroll, event, touch);
    }
    this.listen(scroll, "scroll", () => this.scrolled());
    this.listen(document, "visibilitychange", () => {
      if (document.visibilityState === "hidden") this.flush();
    });
    this.listen(window, "pagehide", () => this.flush());
    this.resize = new ResizeObserver(() => this.follow());
    this.resize.observe(article);
  }

  /** Goes to `offset` and stays there while the layout settles; `undefined` stays at the top. */
  reopen(offset: number | undefined): void {
    this.target = offset;
    this.follow();
  }

  /** Scrolls so that `offset` is a third down the screen: for a Jump to a match. */
  jumpTo(offset: number): void {
    this.userScrolled = this.moved = true;
    this.scrollTo(offset, this.scroll.clientHeight / 3);
  }

  /** Keeps the place now, if the user moved since it was kept. */
  flush(): void {
    clearTimeout(this.timer);
    this.timer = undefined;
    if (!this.moved) return;
    this.moved = false;
    const bounds = this.scroll.getBoundingClientRect();
    const articleBounds = this.article.getBoundingClientRect();
    const x = Math.min(Math.max(articleBounds.left, bounds.left) + 24, bounds.right - 1);
    const offset = offsetAtPoint(this.article, x, bounds.top + margin);
    if (offset !== undefined) this.keep(offset);
  }

  dispose(): void {
    this.flush();
    this.resize.disconnect();
    for (const [target, event, listener] of this.listeners) {
      target.removeEventListener(event, listener);
    }
  }

  private listen(target: EventTarget, event: string, listener: () => void): void {
    target.addEventListener(event, listener, { passive: true });
    this.listeners.push([target, event, listener]);
  }

  private scrolled(): void {
    if (!this.moved) return;
    clearTimeout(this.timer);
    this.timer = setTimeout(() => this.flush(), restDelay);
  }

  private follow(): void {
    if (this.target !== undefined && !this.userScrolled) this.scrollTo(this.target, margin);
  }

  private scrollTo(offset: number, below: number): void {
    const rect = rectAt(this.article, offset);
    if (!rect) return;
    this.scroll.scrollTop += rect.top - this.scroll.getBoundingClientRect().top - below;
  }
}
