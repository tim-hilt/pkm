/**
 * An in-memory WebDAV server for tests, behind a replaced `fetch`: PROPFIND,
 * GET, PUT, MKCOL, MOVE and DELETE with ETags and Basic auth, and switches for
 * rejecting the credentials, answering 507 and failing the network. It
 * serves the folder at `root` (its URL, without a trailing slash).
 */
export class MemoryWebDav {
  /** Every request received, as `METHOD path`. */
  readonly requests: string[] = [];
  /** Authorization headers received. */
  readonly authorizations: (string | null)[] = [];
  /** Answers 401 to everything. */
  rejectCredentials = false;
  /** Answers 507 to anything that writes. */
  full = false;
  /** Throws, as `fetch` does without a network or with CORS refused. */
  offline = false;
  /** Answers 404 to everything, as a web server that isn't WebDAV. */
  notWebDav = false;

  private readonly files = new Map<string, { bytes: Uint8Array; etag: string }>();
  private readonly folders = new Set<string>([""]);
  private version = 0;
  private readonly prefix: string;
  private readonly expected: string;

  constructor(
    readonly root: string,
    username = "user",
    password = "secret",
  ) {
    this.prefix = new URL(root).pathname.replace(/\/$/, "");
    this.expected = `Basic ${btoa(`${username}:${password}`)}`;
  }

  /** The `fetch` to hand to the code under test. */
  readonly fetch = async (input: string | URL, init: RequestInit = {}): Promise<Response> => {
    if (this.offline) throw new TypeError("Failed to fetch");
    const url = new URL(String(input));
    const method = init.method ?? "GET";
    const headers = new Headers(init.headers);
    const path = decodeURIComponent(url.pathname);
    this.requests.push(`${method} ${path}`);
    this.authorizations.push(headers.get("Authorization"));
    if (this.rejectCredentials || headers.get("Authorization") !== this.expected)
      return new Response(null, { status: 401 });
    if (this.notWebDav || !path.startsWith(this.prefix)) return new Response(null, { status: 404 });
    const inside = path.slice(this.prefix.length).replace(/^\/|\/$/g, "");
    const body = init.body ? new Uint8Array(init.body as Uint8Array) : new Uint8Array();
    switch (method) {
      case "PROPFIND":
        return this.propfind(inside, headers.get("Depth") ?? "1");
      case "GET": {
        const file = this.files.get(inside);
        return file ? new Response(file.bytes.slice()) : new Response(null, { status: 404 });
      }
      case "PUT":
        if (this.full) return new Response(null, { status: 507 });
        if (!this.folders.has(parent(inside))) return new Response(null, { status: 409 });
        this.store(inside, body);
        return new Response(null, { status: 201 });
      case "MKCOL":
        if (this.folders.has(inside) || this.files.has(inside))
          return new Response(null, { status: 405 });
        if (this.full) return new Response(null, { status: 507 });
        if (!this.folders.has(parent(inside))) return new Response(null, { status: 409 });
        this.folders.add(inside);
        return new Response(null, { status: 201 });
      case "MOVE": {
        const file = this.files.get(inside);
        const target = new URL(headers.get("Destination")!);
        const to = decodeURIComponent(target.pathname).slice(this.prefix.length).replace(/^\//, "");
        if (!file) return new Response(null, { status: 404 });
        if (!this.folders.has(parent(to))) return new Response(null, { status: 409 });
        this.files.delete(inside);
        this.store(to, file.bytes);
        return new Response(null, { status: 204 });
      }
      case "DELETE":
        if (!this.files.delete(inside)) return new Response(null, { status: 404 });
        return new Response(null, { status: 204 });
      default:
        return new Response(null, { status: 405 });
    }
  };

  /**
   * `fetch` through a browser's HTTP cache, one per Device: as for a real
   * server's `Last-Modified` without `Cache-Control`, a GET is answered from
   * what the cache kept, unless the request's `cache` mode says otherwise; a
   * write to the URL drops it.
   */
  cachingFetch(): (input: string | URL, init?: RequestInit) => Promise<Response> {
    const kept = new Map<string, Uint8Array>();
    return async (input, init = {}) => {
      const url = String(input);
      const method = init.method ?? "GET";
      const cacheable = method === "GET" && (init.cache ?? "default") === "default";
      const hit = kept.get(url);
      if (cacheable && hit) return new Response(hit.slice());
      const response = await this.fetch(input, init);
      if (method === "PUT" || method === "DELETE" || method === "MOVE") kept.delete(url);
      if (!cacheable || !response.ok) return response;
      const bytes = new Uint8Array(await response.arrayBuffer());
      kept.set(url, bytes);
      return new Response(bytes.slice());
    };
  }

  /** The names and contents of the files in the folder `inside` (relative to the root). */
  contents(inside = ""): Map<string, Uint8Array> {
    const found = new Map<string, Uint8Array>();
    for (const [path, { bytes }] of this.files)
      if (parent(path) === inside) found.set(path.slice(inside ? inside.length + 1 : 0), bytes);
    return found;
  }

  /** Puts a file in the folder `inside` as another Device would have. */
  put(inside: string, name: string, bytes: Uint8Array): void {
    this.store(inside ? `${inside}/${name}` : name, bytes);
  }

  /** Whether the folder `inside` exists. */
  hasFolder(inside: string): boolean {
    return this.folders.has(inside);
  }

  private store(path: string, bytes: Uint8Array): void {
    this.files.set(path, { bytes: bytes.slice(), etag: `"${++this.version}"` });
  }

  private propfind(inside: string, depth: string): Response {
    if (!this.folders.has(inside)) return new Response(null, { status: 404 });
    const href = (path: string) =>
      `${this.prefix}/${path}`.split("/").map(encodeURIComponent).join("/").replace(/\/+$/, "");
    const entries = [
      `<response><href>${href(inside)}/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>`,
    ];
    if (depth !== "0") {
      for (const folder of this.folders)
        if (folder && parent(folder) === inside)
          entries.push(
            `<response><href>${href(folder)}/</href><propstat><prop><resourcetype><collection/></resourcetype></prop></propstat></response>`,
          );
      for (const [path, { bytes, etag }] of this.files)
        if (parent(path) === inside)
          entries.push(
            `<response><href>${href(path)}</href><propstat><prop><resourcetype/><getcontentlength>${bytes.length}</getcontentlength><getetag>${etag.replace(/"/g, "&quot;")}</getetag></prop></propstat></response>`,
          );
    }
    return new Response(
      `<?xml version="1.0"?><multistatus xmlns="DAV:">${entries.join("")}</multistatus>`,
      {
        status: 207,
      },
    );
  }
}

function parent(path: string): string {
  const slash = path.lastIndexOf("/");
  return slash < 0 ? "" : path.slice(0, slash);
}
