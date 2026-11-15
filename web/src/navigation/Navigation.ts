/** The Contexts in their fixed order; each one's path is `/<context>`. */
export const contexts = ["tasks", "notes", "drawings", "bookmarks", "books"] as const;
export type Context = (typeof contexts)[number];

/** The Contexts whose Items have a path of their own, `/<context>/<id>`. */
const addressed: readonly Context[] = ["tasks", "drawings"];

/** The browser's History, as far as navigation needs it. */
export interface BrowserHistory {
  /** The path of the current entry. */
  readonly path: string;
  /** Adds an entry. */
  push(path: string): void;
  /** Rewrites the current entry. */
  replace(path: string): void;
  /** Moves `delta` entries (negative: back); `onTraverse` follows. */
  go(delta: number): void;
  /** Calls `listener` after the user moved to another entry (back, forward). */
  onTraverse(listener: () => void): void;
}

/** The Settings pages: `main` is `/settings`, the others `/settings/<page>`. */
export const settingsPages = ["main", "tags", "cloud-backing"] as const;
export type SettingsPage = (typeof settingsPages)[number];

const lastContextKey = "lastContext";

/** Where a path points: a Settings page, or a Context and, in Tasks and Drawings, the Item ID after it. */
type Place = { settings: SettingsPage } | { context: Context; item?: string };

/** The place of a path; `undefined` if it names none. What is below a Context but is no Item of it, or below Settings but is no page, is dropped. */
function placeAt(path: string): Place | undefined {
  const [, first, item, ...deeper] = path.split("/");
  if (first === "settings") {
    const page = settingsPages.find((page) => page !== "main" && page === item);
    return { settings: page && deeper.length === 0 ? page : "main" };
  }
  const context = contexts.find((context) => context === first);
  if (!context) return undefined;
  const isItem = addressed.includes(context) && item && deeper.length === 0;
  return isItem ? { context, item } : { context };
}

function pathOf(place: Place): string {
  if ("settings" in place)
    return place.settings === "main" ? "/settings" : `/settings/${place.settings}`;
  return place.item ? `/${place.context}/${place.item}` : `/${place.context}`;
}

/**
 * Where the user is: a Settings page, or the selected Context and, in Tasks and
 * Drawings, the open Item, kept in step with the address bar. Switching a Context or opening an Item
 * pushes a History entry and back and forward restore both; a path that is
 * none is replaced by the Context's own path (or the last Context's), so no
 * entry is a dead end. The Context last shown is remembered in `remembered`
 * (per Device), and `/` opens on it; Tasks at first.
 *
 * An Item ID from a path is not checked here, as Navigation does not know the
 * Items: whoever does calls `closeItem` for one that is not there.
 * A port of the Swift `AppNavigation`.
 */
export class Navigation {
  /** The Context last shown; also while Settings is. */
  selected!: Context;
  /** The Settings page shown instead of the Context, if any. */
  settings: SettingsPage | undefined;
  /** The Item ID of the open Item in `selected`, if any. */
  item: string | undefined;
  /** Called after `selected`, `settings` or `item` changed. */
  onChange: () => void = () => {};
  /** The Items of the entries `openItem` pushed since the list, so that going back leaves them all. */
  private pushed: string[] = [];
  /** Whether the first Item was opened by a link: no entry of its own to go back to the list. */
  private fromLink = false;

  constructor(
    private readonly history: BrowserHistory,
    private readonly remembered: Pick<Storage, "getItem" | "setItem">,
  ) {
    this.follow();
    history.onTraverse(() => {
      this.follow();
      // Moved within the entries pushed (or out of them, which forgets them).
      const at = this.item === undefined ? -1 : this.pushed.lastIndexOf(this.item);
      this.pushed = this.pushed.slice(0, at + 1);
      if (at < 0) this.fromLink = false;
      this.onChange();
    });
  }

  /** Shows `context`, as a new History entry; with an Item open, its Context shows its list again. */
  select(context: Context): void {
    if (context === this.selected && this.item === undefined && this.settings === undefined) return;
    this.history.push(`/${context}`);
    this.forgetPushed();
    this.show({ context });
    this.onChange();
  }

  /** Shows a Settings page, as a new History entry. */
  openSettings(page: SettingsPage = "main"): void {
    if (page === this.settings) return;
    this.history.push(pathOf({ settings: page }));
    this.forgetPushed();
    this.show({ settings: page });
    this.onChange();
  }

  /** Opens the Item `item` of the selected Context, as a new History entry, on top of the one open. */
  openItem(item: string): void {
    if (item === this.item) return;
    if (this.item !== undefined && this.pushed.length === 0) this.fromLink = true;
    this.history.push(pathOf({ context: this.selected, item }));
    this.pushed.push(item);
    this.show({ context: this.selected, item });
    this.onChange();
  }

  /** Moves to the entry before the current one, which `onTraverse` then follows. */
  back(): void {
    this.history.go(-1);
  }

  /**
   * Leaves the open Item, and the Items opened on top of it, at once: back to
   * the entry the first was opened from, leaving none ahead, or, for one
   * opened by a link, the Context's own path in its place.
   */
  closeItem(): void {
    if (this.item === undefined) return;
    // The browser goes back later; `onTraverse` then finds the state as it is now.
    if (this.pushed.length > 0 && !this.fromLink) this.history.go(-this.pushed.length);
    else this.history.replace(`/${this.selected}`);
    this.forgetPushed();
    this.item = undefined;
    this.onChange();
  }

  /** Takes the place of the current entry, which is rewritten to a path that is one if it is not. */
  private follow(): void {
    const place = placeAt(this.history.path) ?? { context: this.lastContext() };
    if (this.history.path !== pathOf(place)) this.history.replace(pathOf(place));
    this.show(place);
  }

  private forgetPushed(): void {
    this.pushed = [];
    this.fromLink = false;
  }

  private show(place: Place): void {
    if ("settings" in place) {
      this.selected = this.lastContext();
      this.settings = place.settings;
      this.item = undefined;
      return;
    }
    this.selected = place.context;
    this.settings = undefined;
    this.item = place.item;
    this.remembered.setItem(lastContextKey, place.context);
  }

  private lastContext(): Context {
    const last = this.remembered.getItem(lastContextKey);
    return contexts.find((context) => context === last) ?? "tasks";
  }
}
