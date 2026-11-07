import { describe, expect, it } from "vitest";
import { buildMarker, OfflineCache } from "./OfflineCache";

const SCOPE = "https://draw.example/";

/** Just enough of CacheStorage: a Map of Maps keyed by URL. */
class FakeCaches {
  stores = new Map<string, Map<string, Response>>();
  async open(name: string) {
    const store = this.stores.get(name) ?? new Map<string, Response>();
    this.stores.set(name, store);
    return {
      put: async (url: string, response: Response) => void store.set(url, response),
      match: async (url: string) => store.get(url)?.clone(),
    };
  }
  async keys() {
    return [...this.stores.keys()];
  }
  async delete(name: string) {
    return this.stores.delete(name);
  }
}

/** A deployment: files by URL, and a switch to go offline. */
class FakeNetwork {
  online = true;
  requests: string[] = [];
  constructor(public files: Record<string, string>) {}
  fetch = async (request: Request): Promise<Response> => {
    this.requests.push(request.url);
    if (!this.online) throw new TypeError("offline");
    const path = new URL(request.url).pathname;
    const body = this.files[path === "/" ? "/index.html" : path];
    return body === undefined ? new Response("", { status: 404 }) : new Response(body);
  };
}

function setup(version: string, network: FakeNetwork, caches = new FakeCaches()) {
  const offline = new OfflineCache({
    caches: caches as unknown as CacheStorage,
    fetch: network.fetch,
    scope: SCOPE,
    version,
    assets: ["app-1.js", "app-1.wasm"],
  });
  return { offline, caches };
}

// Scripts cannot construct a navigate request; the worker reads only these fields.
const navigate = (path = "") =>
  ({ url: SCOPE + path, method: "GET", mode: "navigate" }) as unknown as Request;
const asset = (name: string) => new Request(SCOPE + name);
const text = async (response: Promise<Response> | undefined) => (await response)!.text();

describe("OfflineCache", () => {
  const files = {
    "/index.html": `<app v1>${buildMarker("v1")}`,
    "/app-1.js": "js",
    "/app-1.wasm": "wasm",
  };

  it("opens the app offline once installed, whatever the query", async () => {
    const network = new FakeNetwork(files);
    const { offline } = setup("v1", network);
    await offline.install();
    network.online = false;

    expect(await text(offline.respond(navigate("?sample")))).toBe(`<app v1>${buildMarker("v1")}`);
    expect(await text(offline.respond(asset("app-1.wasm")))).toBe("wasm");
  });

  it("serves assets from the cache without the network", async () => {
    const network = new FakeNetwork(files);
    const { offline } = setup("v1", network);
    await offline.install();
    network.requests = [];

    await text(offline.respond(asset("app-1.js")));
    expect(network.requests).toEqual([]);
  });

  it("fetches index.html network-first", async () => {
    const network = new FakeNetwork(files);
    const { offline } = setup("v1", network);
    await offline.install();
    network.files = { ...files, "/index.html": `<app v1.1>${buildMarker("v1")}` };

    expect(await text(offline.respond(navigate()))).toBe(`<app v1.1>${buildMarker("v1")}`);
    network.online = false;
    expect(await text(offline.respond(navigate()))).toBe(`<app v1.1>${buildMarker("v1")}`);
  });

  it("keeps the old build working while a newer deployment is live", async () => {
    const network = new FakeNetwork(files);
    const { offline } = setup("v1", network);
    await offline.install();
    network.files = { ...files, "/index.html": `<app v2>${buildMarker("v2")}` };

    expect(await text(offline.respond(navigate()))).toBe(`<app v2>${buildMarker("v2")}`);
    network.online = false;
    expect(await text(offline.respond(navigate()))).toBe(`<app v1>${buildMarker("v1")}`);
  });

  it("replaces the old build with a new one on activation", async () => {
    const caches = new FakeCaches();
    const oldNetwork = new FakeNetwork(files);
    await setup("v1", oldNetwork, caches).offline.install();

    const newNetwork = new FakeNetwork({
      "/index.html": `<app v2>${buildMarker("v2")}`,
      "/app-1.js": "js2",
      "/app-1.wasm": "wasm2",
    });
    const { offline } = setup("v2", newNetwork, caches);
    await offline.install();
    await offline.activate();
    newNetwork.online = false;

    expect(await caches.keys()).toEqual(["precache-v2"]);
    expect(await text(offline.respond(navigate()))).toBe(`<app v2>${buildMarker("v2")}`);
    expect(await text(offline.respond(asset("app-1.js")))).toBe("js2");
  });

  it("fails the install, keeping the old build, if an asset is missing", async () => {
    const network = new FakeNetwork({ "/index.html": "<app>", "/app-1.js": "js" });
    const { offline } = setup("v1", network);
    await expect(offline.install()).rejects.toThrow(/app-1\.wasm/);
  });

  it("leaves other origins and non-GET requests alone", () => {
    const { offline } = setup("v1", new FakeNetwork(files));
    expect(offline.respond(new Request("https://elsewhere.example/x.js"))).toBeUndefined();
    expect(
      offline.respond(new Request(SCOPE + "x", { method: "POST", body: "a" })),
    ).toBeUndefined();
  });
});
