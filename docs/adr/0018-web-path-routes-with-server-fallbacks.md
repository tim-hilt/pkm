# Web path routes, with server and service worker fallbacks

The web app routes by path (`/drawings`), not by hash (`/#/drawings`), so that its addresses are the ones in the spec and match `pkm://drawings` on Apple devices.

A path has no file behind it, so every layer that answers a request falls back to `index.html`:

- **BusyBox `httpd`** (the Docker image): `E404:/index.html` in `httpd.conf`. A first-visit deep link renders the app, though with status 404.
- **The service worker**: a navigation is answered by the network, and by the cached `index.html` when the network fails or answers with an error.
- **The dev server** (`web/scripts/dev.ts`): a `Bun.serve` that answers every path with `index.html`.

Asset URLs are absolute (`publicPath: "/"`), as relative ones would resolve under `/drawings/…`. The app must be served at the root of its origin.

## Considered Options

- **Hash routes**: need no fallback, but the addresses differ from the Apple links and the Item ID would sit behind a `#`.
- **A server that rewrites to `index.html`**: BusyBox `httpd` can't; only its error page can.

## Consequences

- A deep link answers with status 404 on first visit. Browsers render it; crawlers would not index it, which a personal app does not need.
- The app can't be hosted under a sub-path.
