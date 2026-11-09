/** The Contexts in their fixed order; each one's path is `/<context>`. */
export const contexts = ["tasks", "notes", "drawings", "bookmarks", "books"] as const;
export type Context = (typeof contexts)[number];

/** The browser's History, as far as navigation needs it. */
export interface BrowserHistory {
  /** The path of the current entry. */
  readonly path: string;
  /** Adds an entry. */
  push(path: string): void;
  /** Rewrites the current entry. */
  replace(path: string): void;
  /** Calls `listener` after the user moved to another entry (back, forward). */
  onTraverse(listener: () => void): void;
}

const lastContextKey = "lastContext";

/** The Context a path belongs to: its first segment; the rest (an Item) is not known yet. */
function contextAt(path: string): Context | undefined {
  const [, first] = path.split("/");
  return contexts.find((context) => context === first);
}

/**
 * Where the user is: the selected Context, kept in step with the address bar.
 * Switching pushes a History entry and back and forward restore the Context;
 * a path that is no Context (or names a deeper path) is replaced by the
 * Context's own path, so no entry is a dead end. The Context last shown is
 * remembered in `remembered` (per Device), and `/` opens on it; Tasks at first.
 * A port of the Swift `AppNavigation`.
 */
export class Navigation {
  selected: Context;
  /** Called after the selected Context changed. */
  onChange: () => void = () => {};

  constructor(
    private readonly history: BrowserHistory,
    private readonly remembered: Pick<Storage, "getItem" | "setItem">,
  ) {
    this.selected = this.follow();
    history.onTraverse(() => {
      this.selected = this.follow();
      this.onChange();
    });
  }

  /** Shows `context`, as a new History entry. */
  select(context: Context): void {
    if (context === this.selected) return;
    this.history.push(`/${context}`);
    this.selected = context;
    this.remembered.setItem(lastContextKey, context);
    this.onChange();
  }

  /** The Context of the current entry, which is rewritten to its own path if it is not; the last Context if the entry is none. */
  private follow(): Context {
    const context = contextAt(this.history.path) ?? this.lastContext();
    if (this.history.path !== `/${context}`) this.history.replace(`/${context}`);
    this.remembered.setItem(lastContextKey, context);
    return context;
  }

  private lastContext(): Context {
    const last = this.remembered.getItem(lastContextKey);
    return contexts.find((context) => context === last) ?? "tasks";
  }
}
