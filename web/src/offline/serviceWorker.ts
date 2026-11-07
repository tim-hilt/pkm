// The service worker's entry. `scripts/build.ts` bundles it with the build's
// `__BUILD__` (version and precache list) defined.
import { OfflineCache } from "./OfflineCache";

declare const __BUILD__: { version: string; assets: string[] };
// The DOM lib is the project's; only what the worker uses is declared.
interface WorkerEvent extends Event {
  waitUntil(promise: Promise<unknown>): void;
}
interface FetchWorkerEvent extends WorkerEvent {
  request: Request;
  respondWith(response: Promise<Response>): void;
}
declare const self: {
  caches: CacheStorage;
  fetch: typeof fetch;
  registration: { scope: string };
  clients: { claim(): Promise<void> };
  skipWaiting(): Promise<void>;
  addEventListener(type: "install" | "activate", listener: (event: WorkerEvent) => void): void;
  addEventListener(type: "fetch", listener: (event: FetchWorkerEvent) => void): void;
};

const offline = new OfflineCache({
  caches: self.caches,
  fetch: (request) => self.fetch(request),
  scope: self.registration.scope,
  ...__BUILD__,
});

// A new build takes over at once; the open page keeps running what it loaded,
// the next open runs the new build.
self.addEventListener("install", (event) => {
  event.waitUntil(offline.install().then(() => self.skipWaiting()));
});
self.addEventListener("activate", (event) => {
  event.waitUntil(offline.activate().then(() => self.clients.claim()));
});
self.addEventListener("fetch", (event) => {
  const response = offline.respond(event.request);
  if (response) event.respondWith(response);
});
