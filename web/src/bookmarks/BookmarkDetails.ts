import { button, make } from "../Dom";
import { confirmButton, field, modal, textInput } from "../tasks/TaskDialogs";
import type { BookmarkRow, BookmarksModel } from "./BookmarksModel";
import { tagsRow } from "./SaveDialog";

/** What the Details can do besides change the Bookmark: show other places. */
export interface DetailsPlaces {
  /** Reads the Article in the Reader. */
  read(id: string): void;
  /** Shows the page in the browser. */
  openPage(address: string): void;
}

/** Asks before something that can't be undone, then calls `confirmed`. */
function confirm(parent: HTMLElement, question: string, label: string, confirmed: () => void) {
  const ok = confirmButton(label);
  const { dialog, form } = modal(
    parent,
    question,
    [],
    [button("", "Cancel", () => dialog.close()), ok],
  );
  ok.classList.add("danger");
  form.addEventListener("submit", (event) => {
    event.preventDefault();
    dialog.close();
    confirmed();
  });
}

/**
 * Bookmark Details: the Cover, the Title and Tags to change, what the page said about itself
 * (Site, Address, Author, Published, Excerpt, shown as found), when it was Saved, and the actions on
 * the Bookmark: Read, Open Page, Add or Drop Article, Delete. Dropping and Deleting always ask.
 * A dialog over the list; the Title is saved when it is left. `update` follows the model. A port of
 * the Swift `BookmarkDetails`.
 */
export class BookmarkDetails {
  readonly dialog: HTMLDialogElement;
  private readonly title = textInput("");
  private readonly rest = make("div", "tasks-form");
  private addingFailed = false;
  private adding = false;

  constructor(
    private readonly parent: HTMLElement,
    private readonly model: BookmarksModel,
    readonly id: string,
    private readonly places: DetailsPlaces,
    /** Called once the dialog is closed, by the user or because the Bookmark is gone. */
    closed: () => void,
  ) {
    const bookmark = model.bookmark(id)!;
    this.title.ariaLabel = "Title";
    this.title.value = bookmark.title;
    this.title.addEventListener("change", () => this.rename());
    const done = confirmButton("Done");
    const { dialog } = modal(parent, "Bookmark", [field("Title", this.title), this.rest], [done]);
    this.dialog = dialog;
    dialog.classList.add("bookmark-details");
    dialog.addEventListener("close", () => {
      this.rename();
      closed();
    });
    this.update();
  }

  private rename(): void {
    const { model, id } = this;
    const current = model.bookmark(id);
    if (current && this.title.value.trim() !== "" && this.title.value !== current.title) {
      model.rename(id, this.title.value);
    }
  }

  /** Draws everything but the Title again from the model; closes if the Bookmark is gone. */
  update(): void {
    const bookmark = this.model.bookmark(this.id);
    if (!bookmark) return this.dialog.close();
    const { model } = this;
    const row = (label: string, value: string) =>
      value === "" ? [] : [make("dt", "", [label]), make("dd", "", [value])];
    const cover = bookmark.cover ? make("img", "bookmark-cover-large") : undefined;
    if (cover && bookmark.cover) {
      cover.alt = "";
      model.images.show(cover, bookmark.cover);
    }
    this.rest.replaceChildren(
      ...(cover ? [cover] : []),
      tagsRow(model, bookmark.tags, (tags) => void model.setTags(bookmark.id, tags)),
      make("dl", "bookmark-facts", [
        ...row("Site", bookmark.site),
        ...row("Address", bookmark.address),
        ...row("Author", bookmark.author),
        ...row("Published", bookmark.published),
        ...row("Saved", saved(bookmark.saved)),
        ...(bookmark.percent === undefined ? [] : row("Reading Progress", `${bookmark.percent} %`)),
        ...row("Excerpt", bookmark.excerpt),
      ]),
      this.actions(bookmark),
    );
  }

  private actions(bookmark: BookmarkRow): HTMLElement {
    const { model, parent, places } = this;
    const actions = make("div", "bookmark-actions");
    if (bookmark.hasArticle) {
      actions.append(
        button("tasks-back", "Read", () => {
          this.dialog.close();
          places.read(bookmark.id);
        }),
      );
    }
    actions.append(button("tasks-back", "Open Page", () => places.openPage(bookmark.address)));
    if (bookmark.hasArticle) {
      actions.append(
        button("tasks-back", "Drop Article", () =>
          confirm(
            parent,
            "Drop the Article? Its Markup and Reading Progress go with it.",
            "Drop Article",
            () => model.dropArticle(bookmark.id),
          ),
        ),
      );
    } else {
      const add = button("tasks-back", "Add Article", () => void this.addArticle(bookmark.id));
      add.disabled = this.adding;
      actions.append(add);
    }
    actions.append(
      button("tasks-back", "Delete", () =>
        confirm(parent, `Delete “${bookmark.title}”?`, "Delete", () => model.delete(bookmark.id)),
      ),
    );
    return make("div", "", [
      actions,
      ...(this.addingFailed
        ? [make("p", "tasks-hint", ["The page has no text to keep, or couldn't be downloaded."])]
        : []),
    ]);
  }

  private async addArticle(id: string): Promise<void> {
    this.adding = true;
    this.addingFailed = false;
    this.update();
    this.addingFailed = !(await this.model.addArticle(id));
    this.adding = false;
    if (this.dialog.isConnected) this.update();
  }
}

/** The Saved time (UTC ISO 8601) as the user's date and time; the text itself if it isn't one. */
function saved(text: string): string {
  const date = new Date(text);
  return Number.isNaN(date.getTime())
    ? text
    : date.toLocaleString(undefined, {
        dateStyle: "medium",
        timeStyle: "short",
      });
}
