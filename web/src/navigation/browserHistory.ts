import type { BrowserHistory } from "./Navigation";

/** The window's History. */
export function browserHistory(window: Window): BrowserHistory {
  return {
    get path() {
      return window.location.pathname;
    },
    push: (path) => window.history.pushState(null, "", path),
    replace: (path) => window.history.replaceState(null, "", path),
    go: (delta) => window.history.go(delta),
    onTraverse: (listener) => window.addEventListener("popstate", listener),
  };
}
