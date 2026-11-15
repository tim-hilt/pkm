import { type Context, contexts, type Navigation } from "../navigation/Navigation";
import type { SettingsView } from "../settings/SettingsView";

const icon = (paths: string) =>
  `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${paths}</svg>`;

/** What each Context is called and looks like; hand-written, in the spirit of the SF Symbols the Apple app uses. */
/** `placeholder` fills the pane of a Context the web app has no functionality for. */
const presentation: Record<Context, { title: string; icon: string; placeholder?: string }> = {
  tasks: {
    title: "Tasks",
    icon: icon(
      '<path d="M4 6l1.5 1.5L8 5M4 12l1.5 1.5L8 11M4 18l1.5 1.5L8 17M11 6.5h9M11 12.5h9M11 18.5h9"/>',
    ),
  },
  notes: {
    title: "Notes",
    icon: icon('<path d="M6 3h9l4 4v14H6zM15 3v4h4M9 12h7M9 16h7"/>'),
    placeholder: "Coming soon",
  },
  drawings: {
    title: "Drawings",
    icon: icon('<path d="M3 17c3-9 5-12 7-10s-4 11-1 11 4-6 6-6 1 4 3 4 2-2 3-3"/>'),
  },
  bookmarks: {
    title: "Bookmarks",
    icon: icon('<path d="M6 3h12v18l-6-4-6 4z"/>'),
    placeholder: "Coming soon",
  },
  books: {
    title: "Books",
    icon: icon('<path d="M4 4v16M9 4v16M14.5 5l4.5 14.5M4 4h5"/><path d="M14.5 5l3-1 4 15-3 1z"/>'),
    placeholder: "Not available on the web yet",
  },
};
const sidebarIcon = icon('<rect x="3" y="5" width="18" height="14" rx="2"/><path d="M9 5v14"/>');
const gearIcon = icon(
  '<path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z"/><circle cx="12" cy="12" r="3"/>',
);

/**
 * The web app's frame: the five Contexts as a sidebar with Settings pinned to
 * its bottom. At phone widths (a CSS media query, see index.html) the sidebar
 * is hidden until the button at the top left of the Contexts opens it full
 * screen; picking a Context or Settings closes it. Each Context has a pane to
 * fill; the panes of Contexts without functionality hold their placeholder.
 * While an Item fills the window (`setFocused`) the sidebar and its button are
 * hidden. The selected Context is the
 * `Navigation`'s: call `update` after it changed. A port of the Swift
 * `ContentView`.
 */
export class Shell {
  readonly element = document.createElement("div");
  private readonly links = new Map<Context, HTMLAnchorElement>();
  private readonly panes = new Map<Context, HTMLElement>();

  constructor(
    private readonly navigation: Navigation,
    settings: SettingsView,
  ) {
    this.element.className = "shell";
    // Only the phone layout shows the sidebar as a closable overlay; wide screens ignore this.
    const setOpen = (open: boolean) => {
      this.element.classList.toggle("sidebar-open", open);
      menu.setAttribute("aria-expanded", String(open));
    };
    const menu = document.createElement("button");
    menu.type = "button";
    menu.className = "shell-menu";
    menu.setAttribute("aria-label", "Contexts");
    menu.setAttribute("aria-expanded", "false");
    menu.innerHTML = sidebarIcon;
    menu.addEventListener("click", () => setOpen(true));
    const bar = document.createElement("nav");
    bar.className = "shell-bar";
    bar.setAttribute("aria-label", "Contexts");
    const content = document.createElement("div");
    content.className = "shell-content";

    for (const context of contexts) {
      const { title, icon, placeholder } = presentation[context];
      const link = document.createElement("a");
      link.className = "shell-link";
      link.href = `/${context}`;
      link.innerHTML = `${icon}<span>${title}</span>`;
      link.addEventListener("click", (event) => {
        // A plain click switches in place; modified ones keep the browser's way (a new tab).
        if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
        event.preventDefault();
        setOpen(false);
        navigation.select(context);
      });
      this.links.set(context, link);
      bar.append(link);

      const pane = document.createElement("section");
      pane.className = "shell-pane";
      pane.setAttribute("aria-label", title);
      if (placeholder) pane.append(this.placeholder(title, icon, placeholder));
      this.panes.set(context, pane);
      content.append(pane);
    }

    const gear = document.createElement("button");
    gear.type = "button";
    gear.className = "shell-link shell-settings";
    gear.innerHTML = `${gearIcon}<span>Settings</span>`;
    gear.addEventListener("click", () => {
      setOpen(false);
      settings.show();
    });
    bar.append(gear);
    content.append(menu);

    this.element.append(bar, content, settings.element);
    this.update();
  }

  /** Where a Context puts its content. */
  pane(context: Context): HTMLElement {
    return this.panes.get(context)!;
  }

  /** Shows the selected Context. */
  update(): void {
    for (const context of contexts) {
      const selected = context === this.navigation.selected;
      this.panes.get(context)!.hidden = !selected;
      const link = this.links.get(context)!;
      if (selected) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    }
  }

  /** Hides the sidebar and its button while an Item fills the window. */
  setFocused(focused: boolean): void {
    this.element.classList.toggle("focused", focused);
  }

  private placeholder(title: string, icon: string, text: string): HTMLElement {
    const element = document.createElement("div");
    element.className = "shell-placeholder";
    element.setAttribute("role", "status");
    element.innerHTML = `${icon}<strong>${title}</strong><span>${text}</span>`;
    return element;
  }
}
