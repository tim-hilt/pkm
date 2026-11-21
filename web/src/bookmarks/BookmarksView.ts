import { button, make, onNewKey } from "../Dom";
import { RowMenu } from "../tasks/RowMenu";
import { confirmButton, modal } from "../tasks/TaskDialogs";
import { tagLabel } from "../tasks/TagLabel";
import { ArticleReader } from "./ArticleReader";
import { BookmarkDetails } from "./BookmarkDetails";
import type { BookmarkHit, BookmarkRow, BookmarksModel } from "./BookmarksModel";
import { saveDialog } from "./SaveDialog";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;
const articleGlyph = `<svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" role="img" aria-label="Article"><path d="M6 3h9l4 4v14H6zM9 12h7M9 16h7"/></svg>`;

/** How the view moves between the list and a Bookmark's Details, which has an address of its own. */
export interface BookmarkPlaces {
  /** Shows the Details of the Bookmark `id` at its address. */
  openDetails(id: string): void;
  /** Leaves the Details, back to the list's address. */
  closeDetails(): void;
  /** The Reader is in Immersive Mode, or no longer: the window shows only the Article, or its usual frame. */
  immersive(on: boolean): void;
}

/**
 * The Bookmarks Context: its Bookmarks, newest Saved first, each with Cover, Title, Site, Tags and
 * a mark if it has an Article, narrowed to one Tag, searched by words. Clicking one opens its
 * Article in the Reader, or else its page in a new tab; its menu (right click, long press)
 * Edits it (its Details) or Deletes it, after asking. "+" opens the Save dialog. A port of the
 * Swift `BookmarksView`.
 */
export class BookmarksView {
  readonly element = make("main", "bookmarks");
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty", [
    make("strong", "", ["No Bookmarks"]),
    make("span", "", ["Tap + and enter the address of a page to Save it."]),
  ]);
  private readonly noResults = make("div", "tasks-empty");
  private readonly search = make("input", "tasks-input notes-search");
  private readonly tags = make("select", "bookmarks-tags");
  private readonly failure = make("p", "tasks-failure");
  private readonly bar: HTMLElement;
  private readonly scroll: HTMLElement;
  private readonly menu: RowMenu;
  /** The Reader's Bookmark, and the Reader. */
  private reading: { id: string; reader: ArticleReader } | undefined;
  private details: BookmarkDetails | undefined;

  constructor(
    private readonly model: BookmarksModel,
    private readonly places: BookmarkPlaces,
  ) {
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Bookmarks");
    this.menu = new RowMenu(this.element);
    this.search.type = "search";
    this.search.placeholder = "Search";
    this.search.ariaLabel = "Search Bookmarks";
    this.search.addEventListener("input", () => model.search(this.search.value));
    this.tags.ariaLabel = "Tag";
    this.tags.addEventListener("change", () => (model.tagFilter = this.tags.value || undefined));
    const create = button("tasks-new", "", () => this.save());
    create.innerHTML = plusGlyph;
    create.title = create.ariaLabel = "Add Bookmark";
    this.bar = make("div", "bookmarks-bar", [this.search, this.tags]);
    this.scroll = make("div", "tasks-scroll", [this.list, this.empty, this.noResults]);
    this.element.append(this.bar, this.scroll, this.failure, create, this.menu.element);
    onNewKey(this.element, () => this.save());
    model.onChange = () => this.update();
    this.update();
  }

  /** Shows the list, the Reader and the Details as the model has them. */
  update(): void {
    const { model } = this;
    this.menu.hide();
    const searching = model.searchText.trim() !== "";
    this.list.replaceChildren(
      ...(searching
        ? model.results.map((hit) => this.row(hit.bookmark, hit))
        : model.shown.map((bookmark) => this.row(bookmark))),
    );
    this.empty.hidden = searching || model.all.length > 0;
    this.noResults.hidden = !searching || model.results.length > 0;
    this.noResults.replaceChildren(
      make("strong", "", [`No Results for “${model.searchText.trim()}”`]),
    );
    this.showTags();
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showReader();
    this.showDetails();
  }

  private showTags(): void {
    const { model } = this;
    const inUse = model.tagsInUse;
    this.tags.hidden = inUse.length === 0;
    this.tags.replaceChildren(
      new Option("All Bookmarks", ""),
      ...inUse.map((tag) => new Option(tag, tag)),
    );
    this.tags.value = model.tagFilter ?? "";
  }

  private save(): void {
    saveDialog(this.element, this.model, (bookmark) => this.places.openDetails(bookmark.id));
  }

  private row(bookmark: BookmarkRow, found?: BookmarkHit): HTMLElement {
    const { model } = this;
    const cover = make("span", "bookmark-cover");
    if (bookmark.cover) {
      const image = make("img", "");
      image.alt = "";
      image.loading = "lazy";
      image.decoding = "async";
      model.images.show(image, bookmark.cover);
      cover.append(image);
    }
    const colors = new Map(model.knownTags().map((tag) => [tag.name, tag.color]));
    const meta = make("span", "bookmark-meta");
    if (bookmark.hasArticle) meta.insertAdjacentHTML("beforeend", articleGlyph);
    if (bookmark.percent !== undefined) meta.append(make("span", "", [`${bookmark.percent} %`]));
    meta.append(make("span", "bookmark-site", [bookmark.site]));
    const text = make("span", "task-text", [
      make("span", "task-title bookmark-title", [bookmark.title]),
      meta,
      ...(found && found.snippet !== ""
        ? [
            make("span", "task-description bookmark-snippet", [
              found.snippet.slice(0, found.matchStart),
              make("b", "", [found.snippet.slice(found.matchStart, found.matchEnd)]),
              found.snippet.slice(found.matchEnd),
            ]),
          ]
        : []),
      ...(bookmark.tags.length > 0
        ? [
            make(
              "span",
              "task-tags",
              bookmark.tags.map((tag) => tagLabel(tag, colors.get(tag))),
            ),
          ]
        : []),
    ]);
    const open = button("task-open bookmark-open", "", () =>
      model.openBookmark(bookmark, openPage),
    );
    open.append(cover, text);
    const item = make("li", "task-row", [open]);
    this.menu.attach(item, () => [
      { label: "Edit", run: () => this.places.openDetails(bookmark.id) },
      { label: "Delete", danger: true, run: () => this.ask(bookmark) },
    ]);
    item.addEventListener("keydown", (event) => {
      if (event.key === "Backspace" || event.key === "Delete") this.ask(bookmark);
    });
    return item;
  }

  /** Builds the Reader when a Bookmark is opened, and takes it away when it is closed. */
  private showReader(): void {
    const { model } = this;
    const id = model.openId;
    this.element.classList.toggle("has-open", id !== undefined);
    if (id === this.reading?.id) return;
    this.reading?.reader.dispose();
    this.reading?.reader.element.remove();
    this.reading = undefined;
    const bookmark = model.bookmark(id);
    if (!bookmark) return;
    const reader = new ArticleReader(model, bookmark, {
      back: () => model.closeReader(),
      details: () => this.places.openDetails(bookmark.id),
      openPage: () => openPage(bookmark.address),
      immersive: (on) => this.places.immersive(on),
    });
    this.reading = { id: bookmark.id, reader };
    this.element.append(reader.element);
  }

  /** Opens the Details the model names, and closes them when it names none. */
  private showDetails(): void {
    const id = this.model.detailsId;
    if (id === undefined) {
      this.details?.dialog.close();
      this.details = undefined;
      return;
    }
    if (this.details && this.details.id === id) return this.details.update();
    this.details?.dialog.close();
    const details = new BookmarkDetails(
      this.element,
      this.model,
      id,
      {
        read: (bookmark) => this.model.openBookmark(this.model.bookmark(bookmark)!, openPage),
        openPage,
      },
      () => {
        // Closed by the user, or by the model having none to show; not one replaced by another.
        const current = this.details === details;
        if (current) this.details = undefined;
        if (current || this.model.detailsId === undefined) this.places.closeDetails();
      },
    );
    this.details = details;
  }

  /** Deletes the Bookmark after asking. */
  private ask(bookmark: BookmarkRow): void {
    const { model } = this;
    const confirm = confirmButton("Delete");
    confirm.classList.add("danger");
    const { dialog, form } = modal(
      this.element,
      `Delete “${bookmark.title}”?`,
      [],
      [button("", "Cancel", () => dialog.close()), confirm],
    );
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      model.delete(bookmark.id);
      dialog.close();
    });
  }
}

/** Shows a page in a new tab. */
function openPage(address: string): void {
  window.open(address, "_blank", "noopener,noreferrer");
}
