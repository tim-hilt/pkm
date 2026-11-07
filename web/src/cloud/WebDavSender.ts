import type { DavExchange } from "../../generated/excalidraw_native_web.js";

/** Sends one request; `fetch`'s shape, so tests replace it. */
export type Fetch = (input: string, init: RequestInit) => Promise<Response>;

/** How an operation of the WebDAV protocol ended. */
export interface DavResult {
  /** Nothing if it worked, else what went wrong, see `DavOutcome.error`. */
  error?: string;
  message: string;
  /** What a `get` found. */
  data?: Uint8Array;
  /** What a `list` found. */
  blobs: { name: string; marker: string }[];
}

/**
 * Runs an operation of the protocol with `fetch`: sends what it asks until it
 * is over. Every request carries `Authorization: Basic` (the protocol sees to
 * that), so the browser's own password prompt never appears. A `fetch` that
 * throws (no network, an untrusted certificate, CORS) ends it as unreachable
 * or offline. Frees the operation.
 */
export async function runExchange(exchange: DavExchange, send: Fetch): Promise<DavResult> {
  try {
    let request = exchange.start();
    for (;;) {
      let response: Response;
      let body: Uint8Array;
      try {
        const headers = new Headers();
        const flat = request.headers;
        for (let i = 0; i < flat.length; i += 2) headers.set(flat[i], flat[i + 1]);
        const { url, method, body: content } = request;
        request.free();
        response = await send(url, {
          method,
          headers,
          body: content.length > 0 ? (content as BodyInit) : undefined,
          // Credentials are ours; never the browser's.
          credentials: "omit",
        });
        body = new Uint8Array(await response.arrayBuffer());
      } catch {
        exchange.failed();
        break;
      }
      const next = exchange.answer(response.status, body);
      if (!next) break;
      request = next;
    }
    const outcome = exchange.outcome();
    const result: DavResult = {
      error: outcome.error,
      message: outcome.message,
      data: outcome.data,
      blobs: outcome.names.map((name, i) => ({ name, marker: outcome.markers[i] })),
    };
    outcome.free();
    return result;
  } finally {
    exchange.free();
  }
}
