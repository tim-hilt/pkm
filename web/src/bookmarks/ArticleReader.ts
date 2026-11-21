import { make, button } from "../Dom";
import { descriptionNodes } from "../tasks/DescriptionText";
import type { BookmarkRow, BookmarksModel } from "./BookmarksModel";

/**
 * The Reader: a Bookmark's Article, read-only, in the app's Markdown styling (the markers hidden
 * throughout, as the Description's rendering draws them), following Appearance through the
 * colours the page sets. Its images are shown from the kept files; its links open in a new tab.
 */
export class ArticleReader {
  readonly element = make("div", "bookmarks-reader");

  constructor(
    model: BookmarksModel,
    bookmark: BookmarkRow,
    actions: { back: () => void; details: () => void; openPage: () => void },
  ) {
    const article = make("div", "tasks-description bookmark-article");
    article.setAttribute("role", "article");
    article.append(...descriptionNodes(model.article(bookmark.id) ?? "", undefined, model.images));
    article.addEventListener("click", (event) => {
      const href = (event.target as Element).closest<HTMLElement>("a[data-href]")?.dataset.href;
      if (!href) return;
      event.preventDefault();
      window.open(href, "_blank", "noopener,noreferrer");
    });
    this.element.append(
      make("div", "bookmarks-bar", [
        button("tasks-back", "Bookmarks", actions.back),
        make("div", "notes-bar-end", [
          button("tasks-back", "Open Page", actions.openPage),
          button("tasks-back", "Details", actions.details),
        ]),
      ]),
      make("div", "tasks-scroll", [make("div", "bookmark-reader-page", [article])]),
    );
    this.element.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !document.querySelector("dialog[open]")) actions.back();
    });
  }
}
