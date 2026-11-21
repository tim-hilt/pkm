import { button, make } from "../Dom";
import { descriptionNodes } from "../tasks/DescriptionText";
import { ArticleInk, type Passage } from "./ArticleInk";
import { offsetOf, rangeOf, rectAt } from "./ArticleOffsets";
import type { ArticleMarkup, BookmarkRow, BookmarksModel } from "./BookmarksModel";
import { FindBar } from "./FindBar";
import { MarkupPopover } from "./MarkupPopover";
import { ReadingSpot } from "./ReadingSpot";
import { SelectionToolbar } from "./SelectionToolbar";

/** What the Reader asks of the view around it. */
export interface ReaderActions {
  back(): void;
  details(): void;
  openPage(): void;
  /** Immersive Mode is on or off: the view hides what surrounds the Reader, or shows it again. */
  immersive(on: boolean): void;
}

/**
 * The Reader: a Bookmark's Article in the app's Markdown styling (the markers hidden throughout, as
 * the Description's rendering draws them), following Appearance through the colours the page sets.
 * Its images are shown from the kept files; its links open in a new tab. The Article is never
 * edited, but marked up: selecting text offers Highlight (in the Default Highlight Color) and Comment;
 * pressing a Highlight or a note beside the text opens it to recolor, comment on or delete. It
 * reopens where it was left (Reading Progress), a press on the text hides everything but the
 * Article (Immersive Mode), and Find is Article Search. Call `dispose` when it goes.
 */
export class ArticleReader {
  readonly element = make("div", "bookmarks-reader");
  private readonly id: string;
  private readonly markdown: string;
  private readonly article = make("div", "tasks-description bookmark-article");
  private readonly notes = make("div", "reader-notes");
  private readonly scroll: HTMLElement;
  private readonly ink: ArticleInk;
  private readonly spot: ReadingSpot;
  private readonly popover = new MarkupPopover();
  private readonly find: FindBar;
  private readonly selection: SelectionToolbar;
  private readonly immersiveButton: HTMLButtonElement;
  private readonly cleanup: (() => void)[] = [];
  private markup: ArticleMarkup = { highlights: [], comments: [] };
  private matches: Passage[] = [];
  private current = 0;
  private isImmersive = false;

  constructor(
    private readonly model: BookmarksModel,
    bookmark: BookmarkRow,
    private readonly actions: ReaderActions,
  ) {
    this.id = bookmark.id;
    this.markdown = model.article(bookmark.id) ?? "";
    const { article } = this;
    article.setAttribute("role", "article");
    article.append(...descriptionNodes(this.markdown, undefined, model.images));
    article.addEventListener("click", (event) => this.pressed(event));

    this.find = new FindBar({
      query: (text) => this.search(text),
      step: (direction) => this.step(direction),
      close: () => this.closeFind(),
    });
    this.selection = new SelectionToolbar({
      highlight: () => void this.highlightSelection(),
      comment: () => this.commentOnSelection(),
    });
    this.immersiveButton = button("tasks-back", "Immersive", () => this.setImmersive(true));
    const find = button("tasks-back", "Find", () => this.find.open());
    find.title = "Find in Article";
    this.scroll = make("div", "tasks-scroll", [
      make("div", "bookmark-reader-page", [article, this.notes]),
    ]);
    this.element.append(
      make("div", "bookmarks-bar", [
        button("tasks-back", "Bookmarks", actions.back),
        make("div", "notes-bar-end", [
          find,
          this.immersiveButton,
          button("tasks-back", "Open Page", actions.openPage),
          button("tasks-back", "Details", actions.details),
        ]),
      ]),
      this.scroll,
      this.find.element,
      this.selection.element,
      this.popover.element,
    );

    this.ink = new ArticleInk(article);
    this.spot = new ReadingSpot(this.scroll, article, (offset) =>
      model.saveProgress(this.id, offset),
    );
    this.spot.reopen(bookmark.offset);
    this.refreshMarkup();

    // Notes sit beside the text, which moves when Images load or the window changes.
    const layout = new ResizeObserver(() => this.layoutNotes());
    layout.observe(article);
    this.cleanup.push(() => layout.disconnect());
    this.listen(document, "selectionchange", () => this.selectionChanged());
    this.listen(document, "keydown", (event) => {
      const key = event as KeyboardEvent;
      this.key(key);
      if ((key.metaKey || key.ctrlKey) && key.key === "f" && this.element.isConnected) {
        key.preventDefault();
        this.find.open();
      }
    });
  }

  /** Keeps Reading Progress, takes down what the Reader drew and lets the view show its surroundings. */
  dispose(): void {
    this.spot.dispose();
    this.ink.dispose();
    this.popover.dismiss();
    for (const undo of this.cleanup) undo();
    if (this.isImmersive) this.actions.immersive(false);
  }

  private listen(target: EventTarget, event: string, listener: (event: Event) => void): void {
    target.addEventListener(event, listener);
    this.cleanup.push(() => target.removeEventListener(event, listener));
  }

  private key(event: KeyboardEvent): void {
    if (
      event.key !== "Escape" ||
      !this.element.isConnected ||
      document.querySelector("dialog[open]")
    )
      return;
    if (this.isImmersive) this.setImmersive(false);
    else this.actions.back();
  }

  private setImmersive(on: boolean): void {
    this.isImmersive = on;
    this.element.classList.toggle("is-immersive", on);
    this.actions.immersive(on);
  }

  /** A press on the text: a link opens, a Highlight opens, else Immersive Mode is toggled. */
  private pressed(event: MouseEvent): void {
    const target = event.target as Element;
    const href = target.closest<HTMLElement>("a[data-href]")?.dataset.href;
    if (href) {
      event.preventDefault();
      window.open(href, "_blank", "noopener,noreferrer");
      return;
    }
    if (!getSelection()?.isCollapsed) return;
    const highlight = this.markup.highlights.find((h) =>
      [...rangeOf(this.article, h.start, h.end).getClientRects()].some(
        (r) =>
          r.left <= event.clientX &&
          event.clientX <= r.right &&
          r.top <= event.clientY &&
          event.clientY <= r.bottom,
      ),
    );
    if (highlight) void this.openHighlight(highlight.id, new DOMRect(event.clientX, event.clientY));
    else this.setImmersive(!this.isImmersive);
  }

  // Markup

  private refreshMarkup(): void {
    this.markup = this.model.markup(this.id);
    this.ink.showHighlights(this.markup.highlights);
    this.layoutNotes();
  }

  /** The selected text of the Article as offsets into its Markdown, if any is selected. */
  private selected(): { start: number; end: number; rect: DOMRect } | undefined {
    const selection = getSelection();
    if (!selection || selection.isCollapsed || selection.rangeCount === 0) return undefined;
    const range = selection.getRangeAt(0);
    if (!this.article.contains(range.commonAncestorContainer)) return undefined;
    const start = offsetOf(this.article, range.startContainer, range.startOffset);
    const end = offsetOf(this.article, range.endContainer, range.endOffset);
    const rects = range.getClientRects();
    return { start, end, rect: rects[rects.length - 1] ?? range.getBoundingClientRect() };
  }

  private selectionChanged(): void {
    const selected = this.selected();
    if (selected) this.selection.show(selected.rect);
    else this.selection.hide();
  }

  private async highlightSelection(): Promise<void> {
    const selected = this.selected();
    if (!selected) return;
    const { colors, defaultId } = await this.model.highlightColors();
    const rgb = colors.find((color) => color.id === defaultId)?.rgb ?? colors[0]!.rgb;
    const { start, end } = selected;
    this.model.addHighlight(this.id, start, end, this.markdown.slice(start, end), rgb);
    getSelection()?.removeAllRanges();
    this.refreshMarkup();
  }

  /** Puts a Comment where the selection ends and opens it for typing. */
  private commentOnSelection(): void {
    const selected = this.selected();
    if (!selected) return;
    const comment = this.model.addComment(this.id, selected.end);
    getSelection()?.removeAllRanges();
    this.openComment(comment.id, selected.rect);
  }

  private async openHighlight(id: string, anchor: DOMRect): Promise<void> {
    const highlight = this.markup.highlights.find((h) => h.id === id);
    if (!highlight) return;
    const { colors } = await this.model.highlightColors();
    this.popover.open(anchor, {
      color: {
        current: highlight.rgb,
        choices: colors,
        set: (rgb) => {
          this.model.setHighlightColor(this.id, id, rgb);
          this.refreshMarkup();
        },
      },
      comment: highlight.comment ?? "",
      setComment: (text) => {
        this.model.setHighlightComment(this.id, id, text);
        this.refreshMarkup();
      },
      remove: () => {
        this.model.removeHighlight(this.id, id);
        this.refreshMarkup();
      },
    });
  }

  /** Opens the Comment on a spot, which is kept once it has text. */
  private openComment(id: string, anchor: DOMRect): void {
    this.popover.open(anchor, {
      comment: this.markup.comments.find((c) => c.id === id)?.text ?? "",
      setComment: (text) => {
        this.model.setCommentText(this.id, id, text);
        this.refreshMarkup();
      },
      remove: () => {
        this.model.removeComment(this.id, id);
        this.refreshMarkup();
      },
    });
  }

  /** One button beside the text for each Comment, at the line it is on. */
  private layoutNotes(): void {
    const page = this.notes.getBoundingClientRect();
    const notes: HTMLElement[] = [];
    const note = (comment: string, offset: number, open: (anchor: DOMRect) => void) => {
      const rect = rectAt(this.article, offset);
      if (!rect) return;
      // Where the text is when pressed, which is not where it was when laid out.
      const element = button("reader-note", "", () => open(rectAt(this.article, offset) ?? rect));
      element.ariaLabel = element.title = `Comment: ${comment}`;
      element.style.top = `${rect.top - page.top}px`;
      notes.push(element);
    };
    for (const h of this.markup.highlights) {
      if (h.comment) note(h.comment, h.start, (anchor) => void this.openHighlight(h.id, anchor));
    }
    for (const c of this.markup.comments) {
      note(c.text, c.offset, (anchor) => this.openComment(c.id, anchor));
    }
    this.notes.replaceChildren(...notes);
  }

  // Article Search

  private search(text: string): void {
    this.matches = this.model.searchArticle(this.id, text);
    this.current = 0;
    this.showMatches();
    if (this.matches[0]) this.spot.jumpTo(this.matches[0].start);
  }

  private step(direction: 1 | -1): void {
    const { length } = this.matches;
    if (length === 0) return;
    this.current = (this.current + direction + length) % length;
    this.showMatches();
    this.spot.jumpTo(this.matches[this.current]!.start);
  }

  private closeFind(): void {
    this.matches = [];
    this.showMatches();
    this.find.close();
  }

  private showMatches(): void {
    this.ink.showFound(this.matches, this.current);
    this.find.show(this.current, this.matches.length);
  }
}
