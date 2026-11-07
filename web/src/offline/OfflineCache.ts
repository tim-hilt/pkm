// The offline half of the PWA: the hashed assets are precached per build and
// served cache-first, `index.html` is fetched network-first. A new build has a
// new `version`, hence a new cache; the old ones are deleted when it activates.
// Nothing here relies on `Cache-Control` headers.

const CACHE_PREFIX = "precache-";

/** The tag the build puts into `index.html`, naming the build it belongs to. */
export function buildMarker(version: string): string {
  return `<meta name="build" content="${version}">`;
}

export interface OfflineCacheOptions {
  caches: CacheStorage;
  fetch: (request: Request) => Promise<Response>;
  /** The service worker's scope, an absolute URL ending in "/". */
  scope: string;
  /** Identifies the build; changes whenever any precached file changes. */
  version: string;
  /** The hashed assets, relative to the scope. */
  assets: string[];
}

export class OfflineCache {
  private readonly cacheName: string;
  private readonly indexUrl: string;

  constructor(private readonly options: OfflineCacheOptions) {
    this.cacheName = CACHE_PREFIX + options.version;
    this.indexUrl = new URL("index.html", options.scope).href;
  }

  /** Precaches the assets and the current `index.html`, all or nothing. */
  async install(): Promise<void> {
    const cache = await this.options.caches.open(this.cacheName);
    const urls = [
      this.indexUrl,
      ...this.options.assets.map((a) => new URL(a, this.options.scope).href),
    ];
    await Promise.all(
      urls.map(async (url) => {
        const response = await this.options.fetch(new Request(url, { cache: "reload" }));
        if (!response.ok) throw new Error(`precache of ${url} failed: ${response.status}`);
        await cache.put(url, response);
      }),
    );
  }

  /** Deletes the caches of earlier builds. */
  async activate(): Promise<void> {
    const names = await this.options.caches.keys();
    await Promise.all(
      names
        .filter((name) => name.startsWith(CACHE_PREFIX) && name !== this.cacheName)
        .map((name) => this.options.caches.delete(name)),
    );
  }

  /** The response for a request, or undefined if the worker leaves it alone. */
  respond(request: Request): Promise<Response> | undefined {
    if (request.method !== "GET") return undefined;
    const url = new URL(request.url);
    if (url.origin !== new URL(this.options.scope).origin) return undefined;
    if (request.mode === "navigate") return this.networkFirst(request);
    return this.cacheFirst(request);
  }

  // Every navigation within the scope is the app, whatever its query. An
  // error page (the tunnel up, the server behind it down) counts as no
  // network: the cached app opens instead.
  private async networkFirst(request: Request): Promise<Response> {
    const cache = await this.options.caches.open(this.cacheName);
    let response: Response;
    try {
      response = await this.options.fetch(request);
    } catch (error) {
      const cached = await cache.match(this.indexUrl);
      if (cached) return cached;
      throw error;
    }
    if (!response.ok) return (await cache.match(this.indexUrl)) ?? response;
    // Only an index of this build is kept: one of a newer deployment names
    // assets this cache does not hold, and the worker of that build will
    // keep it once it has taken over.
    if ((await response.clone().text()).includes(buildMarker(this.options.version))) {
      await cache.put(this.indexUrl, response.clone());
    }
    return response;
  }

  private async cacheFirst(request: Request): Promise<Response> {
    const cache = await this.options.caches.open(this.cacheName);
    return (await cache.match(request.url)) ?? this.options.fetch(request);
  }
}
