/** A page a bookmarklet hands over: the tab's HTML as the user sees it, and its Address. */
export interface HandedPage {
  address: string;
  html: string;
}

/** What the bookmarklet and the app say to each other (ADR-0033). */
const kind = "pkm-save";
const ready = "pkm-ready";

/** The most HTML taken from a tab; the same cap as a download. */
const limit = 10_000_000;

/** The page in `data` if it is exactly the bookmarklet's message, else `undefined`. */
export function handedPage(data: unknown): HandedPage | undefined {
  if (typeof data !== "object" || data === null) return undefined;
  const keys = Object.keys(data).sort().join();
  if (keys !== "address,html,type") return undefined;
  const { type, address, html } = data as Record<string, unknown>;
  if (type !== kind || typeof address !== "string" || typeof html !== "string") return undefined;
  if (address === "" || html.length > limit) return undefined;
  return { address, html };
}

/** Where the app is reached from, as far as the Save route needs it. */
export interface SaveWindow {
  readonly opener: { postMessage(message: unknown, targetOrigin: string): void } | null;
  addEventListener(type: "message", listener: (event: MessageEvent) => void): void;
}

/**
 * The Save route: a window the bookmarklet opened says it is ready, then takes the one message
 * the bookmarklet sends and calls `accept` with its page. Any other message, from any other
 * window, is ignored.
 */
export function listenForHandedPages(win: SaveWindow, accept: (page: HandedPage) => void): void {
  const opener = win.opener;
  if (!opener) return;
  win.addEventListener("message", (event) => {
    if (event.source !== opener) return;
    const page = handedPage(event.data);
    if (page) accept(page);
  });
  // The opener's origin is unknown, and this says nothing but that the app is there.
  opener.postMessage(ready, "*");
}

/** The bookmarklet for the app at `origin`: opens the app, waits for it, then hands it the tab's page. */
export function bookmarklet(origin: string): string {
  const o = JSON.stringify(origin);
  return (
    `javascript:(()=>{const o=${o},w=open(o+"/bookmarks");` +
    `addEventListener("message",function f(e){if(e.source===w&&e.origin===o&&e.data===${JSON.stringify(ready)}){` +
    `removeEventListener("message",f);` +
    `w.postMessage({type:${JSON.stringify(kind)},address:location.href,html:document.documentElement.outerHTML},o)}})})()`
  );
}
