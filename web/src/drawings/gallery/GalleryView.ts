import type { Entry } from "./DrawingStore";
import type { Gallery } from "./Gallery";
import { type CloudBacking, statusText } from "../../cloud/CloudBacking";

let nextDialog = 0;

type Tag = keyof HTMLElementTagNameMap;

function make<K extends Tag>(
  tag: K,
  className: string,
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  element.className = className;
  element.append(...children);
  return element;
}

function button(className: string, label: string, onClick: () => void): HTMLButtonElement {
  const element = make("button", className, [label]);
  element.type = "button";
  element.addEventListener("click", onClick);
  return element;
}

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;
const moreGlyph = `<svg viewBox="0 0 24 24" width="20" height="20" fill="currentColor" aria-hidden="true"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>`;

/** One Drawing's card: its thumbnail, name and date, and its menu. */
class Card {
  readonly element = make("li", "gallery-card");
  private readonly open: HTMLButtonElement;
  private readonly thumb = make("span", "gallery-thumb");
  private readonly image = document.createElement("img");
  private readonly name = make("span", "gallery-name");
  private readonly date = make("time", "gallery-date");
  private readonly more = make("button", "gallery-more");
  private readonly menu = make("div", "gallery-menu");
  private shown: Entry | undefined;
  private imageUrl: string | undefined;
  /** Counts requests, so that a late answer to an old one is dropped. */
  private request = 0;
  private visible = false;
  private closeMenu: (() => void) | undefined;

  constructor(
    private readonly gallery: Gallery,
    private readonly isDark: () => boolean,
    private readonly actions: { rename(e: Entry): void; remove(e: Entry): void },
    entry: Entry,
  ) {
    this.image.alt = "";
    this.image.decoding = "async";
    this.image.hidden = true;
    this.thumb.append(this.image);
    this.open = make("button", "gallery-open", [this.thumb, this.name, this.date]);
    this.open.type = "button";
    this.open.addEventListener("click", () => void gallery.openDrawing(this.shown!.name));

    this.more.type = "button";
    this.more.innerHTML = moreGlyph;
    this.more.setAttribute("aria-haspopup", "menu");
    this.more.setAttribute("aria-expanded", "false");
    this.more.addEventListener("click", () =>
      this.menu.hidden ? this.showMenu() : this.hideMenu(),
    );

    this.menu.hidden = true;
    this.menu.setAttribute("role", "menu");
    const item = (label: string, run: () => void, className = "") => {
      const element = button(`gallery-menu-item ${className}`, label, () => {
        this.hideMenu();
        this.more.focus();
        run();
      });
      element.setAttribute("role", "menuitem");
      return element;
    };
    this.menu.append(
      item("Rename", () => this.actions.rename(this.shown!)),
      item("Duplicate", () => void gallery.duplicateDrawing(this.shown!.name)),
      item("Delete", () => this.actions.remove(this.shown!), "danger"),
    );
    this.menu.addEventListener("keydown", (event) => this.menuKey(event));
    this.element.append(this.open, this.more, this.menu);
    this.update(entry);
  }

  /** Shows `entry`: another date (the Drawing changed) or appearance asks for a new thumbnail. */
  update(entry: Entry): void {
    const changed = !this.shown || this.shown.modified !== entry.modified;
    this.shown = entry;
    this.name.textContent = entry.name;
    this.date.textContent = new Date(entry.modified).toLocaleString(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    });
    this.date.dateTime = new Date(entry.modified).toISOString();
    this.more.setAttribute("aria-label", `Actions for ${entry.name}`);
    if (changed) this.loadThumbnail();
  }

  /** Asks for the thumbnail again, as the appearance it is made for changed. */
  reloadThumbnail(): void {
    this.loadThumbnail();
  }

  /** Cards outside the screen drop their picture: the store still has it for when they come back. */
  setVisible(visible: boolean): void {
    this.visible = visible;
    if (visible) this.loadThumbnail();
    else this.dropImage();
  }

  dispose(): void {
    this.request++;
    this.dropImage();
    this.hideMenu();
  }

  private loadThumbnail(): void {
    if (!this.visible || !this.shown) return;
    const request = ++this.request;
    void this.gallery.thumbnails.thumbnail(this.shown, this.isDark()).then((thumbnail) => {
      if (request !== this.request) return;
      this.dropImage();
      this.thumb.classList.toggle("unreadable", thumbnail.kind === "unreadable");
      this.thumb.dataset.state = thumbnail.kind;
      if (thumbnail.kind === "image") {
        this.imageUrl = URL.createObjectURL(
          new Blob([thumbnail.png as Uint8Array<ArrayBuffer>], { type: "image/png" }),
        );
        this.image.src = this.imageUrl;
        this.image.hidden = false;
      } else if (thumbnail.kind === "unreadable") {
        // A malformed file is shown as such; it is left as it is.
        this.thumb.dataset.message = "Can’t be read";
      }
    });
  }

  private dropImage(): void {
    this.image.hidden = true;
    this.image.removeAttribute("src");
    if (this.imageUrl) URL.revokeObjectURL(this.imageUrl);
    this.imageUrl = undefined;
  }

  private showMenu(): void {
    this.menu.hidden = false;
    this.more.setAttribute("aria-expanded", "true");
    const away = (event: Event) => {
      if (!this.element.contains(event.target as Node)) this.hideMenu();
    };
    document.addEventListener("pointerdown", away, true);
    this.closeMenu = () => document.removeEventListener("pointerdown", away, true);
    this.menu.querySelector<HTMLElement>("[role=menuitem]")?.focus();
  }

  private hideMenu(): void {
    this.closeMenu?.();
    this.closeMenu = undefined;
    this.menu.hidden = true;
    this.more.setAttribute("aria-expanded", "false");
  }

  private menuKey(event: KeyboardEvent): void {
    const items = [...this.menu.querySelectorAll<HTMLElement>("[role=menuitem]")];
    const at = items.indexOf(document.activeElement as HTMLElement);
    if (event.key === "Escape") {
      this.hideMenu();
      this.more.focus();
    } else if (event.key === "ArrowDown") items[(at + 1) % items.length].focus();
    else if (event.key === "ArrowUp") items[(at - 1 + items.length) % items.length].focus();
    else return;
    event.preventDefault();
  }
}

/**
 * The Gallery as a grid of Drawings, last modified first, with a button to
 * create a new one. A card shows the Drawing's thumbnail, which is made when
 * the card comes into view and again after the Drawing changed, with the name
 * and date below it. A card's menu renames, duplicates and deletes the
 * Drawing; deleting asks first. A port of the Swift `GalleryView`, in native
 * elements (buttons, a list, modal dialogs) so that it is labelled and
 * reachable by keyboard.
 */
export class GalleryView {
  readonly element = make("main", "gallery");
  private readonly grid = make("ul", "gallery-grid");
  private readonly empty = make("div", "gallery-empty", [
    make("strong", "", ["No Drawings"]),
    make("span", "", ["Tap + to start sketching."]),
  ]);
  private readonly cards = new Map<string, Card>();
  private readonly observer: IntersectionObserver;
  private readonly cardOf = new Map<Element, Card>();

  private readonly renameDialog = make("dialog", "gallery-dialog");
  private readonly renameField = document.createElement("input");
  private renaming: Entry | undefined;
  private readonly deleteDialog = make("dialog", "gallery-dialog");
  private readonly deleteTitle = make("h2", "");
  private deleting: Entry | undefined;
  private readonly failureDialog = make("dialog", "gallery-dialog");
  private readonly failureText = make("p", "");

  private readonly syncText = make("p", "gallery-sync");
  private readonly syncWarning = make("span", "gallery-sync-warning");
  private stopSyncListening: (() => void) | undefined;

  /** `isDark` tells which appearance the thumbnails are made for; `cloud` tells what Sync is doing, shown as "Syncing N of M Drawings" and a warning icon. */
  constructor(
    private readonly gallery: Gallery,
    private readonly isDark: () => boolean = () => false,
    private readonly cloud?: Pick<CloudBacking, "syncReport" | "hasProblem" | "onSyncChange">,
  ) {
    const create = button("gallery-new", "", () => void gallery.createDrawing());
    create.innerHTML = plusGlyph;
    create.setAttribute("aria-label", "New Drawing");
    create.title = "New Drawing";

    this.grid.setAttribute("aria-label", "Drawings");
    this.empty.setAttribute("role", "status");
    this.element.append(this.grid, this.empty, create);
    if (cloud) {
      this.syncText.setAttribute("role", "status");
      this.syncWarning.textContent = "⚠";
      this.syncWarning.setAttribute("role", "img");
      this.element.prepend(this.syncText);
      this.element.append(this.syncWarning);
      this.stopSyncListening = cloud.onSyncChange(() => this.showSync());
      this.showSync();
    }
    this.buildDialogs();

    this.observer = new IntersectionObserver((records) => {
      for (const { target, isIntersecting } of records) {
        this.cardOf.get(target)?.setVisible(isIntersecting);
      }
    });
    this.update();
  }

  /** Shows the Drawings and failure of the Gallery as they are now. */
  update(): void {
    const listed = new Set(this.gallery.drawings.map((entry) => entry.name));
    for (const [name, card] of this.cards) {
      if (listed.has(name)) continue;
      this.observer.unobserve(card.element);
      this.cardOf.delete(card.element);
      card.dispose();
      card.element.remove();
      this.cards.delete(name);
    }
    const ordered: HTMLElement[] = [];
    for (const entry of this.gallery.drawings) {
      let card = this.cards.get(entry.name);
      if (card) card.update(entry);
      else {
        card = new Card(this.gallery, this.isDark, this.cardActions, entry);
        this.cards.set(entry.name, card);
        this.cardOf.set(card.element, card);
        this.observer.observe(card.element);
      }
      ordered.push(card.element);
    }
    this.grid.replaceChildren(...ordered);
    this.empty.hidden = ordered.length > 0;

    if (this.gallery.failure !== undefined) {
      this.failureText.textContent = this.gallery.failure;
      if (!this.failureDialog.open) this.failureDialog.showModal();
    } else if (this.failureDialog.open) this.failureDialog.close();
  }

  /** The appearance the thumbnails are made for changed: they are shown again, the old ones staying until the new are ready. */
  appearanceChanged(): void {
    for (const card of this.cards.values()) card.reloadThumbnail();
  }

  /** "Syncing 3 of 40 Drawings" while Drawings transfer, a warning while Sync is blocked. */
  private showSync(): void {
    const state = this.cloud!.syncReport;
    const text = state.status === "syncing" && state.progress ? statusText(state, "") : "";
    this.syncText.textContent = text;
    this.syncText.hidden = text === "";
    this.syncWarning.hidden = !this.cloud!.hasProblem;
    this.syncWarning.setAttribute("aria-label", statusText(state, "The server"));
    this.syncWarning.title = statusText(state, "The server");
  }

  dispose(): void {
    this.stopSyncListening?.();
    this.observer.disconnect();
    for (const card of this.cards.values()) card.dispose();
    this.cards.clear();
  }

  private readonly cardActions = {
    rename: (entry: Entry) => {
      this.renaming = entry;
      this.renameField.value = entry.name;
      this.renameDialog.showModal();
      this.renameField.select();
    },
    remove: (entry: Entry) => {
      this.deleting = entry;
      this.deleteTitle.textContent = `Delete “${entry.name}”?`;
      this.deleteDialog.showModal();
    },
  };

  private buildDialogs(): void {
    const labelled = (dialog: HTMLDialogElement, title: HTMLElement) => {
      title.id = `gallery-dialog-title-${nextDialog++}`;
      dialog.setAttribute("aria-labelledby", title.id);
    };
    const cancel = (dialog: HTMLDialogElement, label = "Cancel") =>
      button("", label, () => dialog.close());

    // Rename
    const renameTitle = make("h2", "", ["Rename Drawing"]);
    this.renameField.type = "text";
    this.renameField.setAttribute("aria-label", "Name");
    this.renameField.autocomplete = "off";
    const rename = make("button", "primary", ["Rename"]);
    rename.type = "submit";
    const renameForm = make("form", "", [
      renameTitle,
      this.renameField,
      make("div", "gallery-dialog-buttons", [cancel(this.renameDialog), rename]),
    ]);
    renameForm.method = "dialog";
    renameForm.addEventListener("submit", () => {
      if (this.renaming)
        void this.gallery.renameDrawing(this.renaming.name, this.renameField.value);
      this.renaming = undefined;
    });
    this.renameDialog.append(renameForm);
    labelled(this.renameDialog, renameTitle);

    // Delete
    const remove = button("primary danger", "Delete", () => {
      if (this.deleting) void this.gallery.deleteDrawing(this.deleting.name);
      this.deleting = undefined;
      this.deleteDialog.close();
    });
    this.deleteDialog.setAttribute("role", "alertdialog");
    this.deleteDialog.append(
      this.deleteTitle,
      make("p", "", ["This Drawing will be deleted for good."]),
      make("div", "gallery-dialog-buttons", [cancel(this.deleteDialog), remove]),
    );
    labelled(this.deleteDialog, this.deleteTitle);

    // Failure
    const ok = button("primary", "OK", () => this.failureDialog.close());
    this.failureDialog.setAttribute("role", "alertdialog");
    this.failureDialog.append(this.failureText, make("div", "gallery-dialog-buttons", [ok]));
    this.failureDialog.setAttribute("aria-label", "Something went wrong");
    this.failureDialog.addEventListener("close", () => {
      if (this.gallery.failure !== undefined) this.gallery.dismissFailure();
    });

    this.element.append(this.renameDialog, this.deleteDialog, this.failureDialog);
  }
}
