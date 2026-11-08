/** What a tab tells the others (ADR-0016). */
export type TabMessage =
  | { kind: "changed"; names: string[] }
  | { kind: "connected" }
  | { kind: "disconnected" };

/**
 * What the tabs of one browser share: a lock so that only one runs a Sync
 * step at a time, and a channel to tell each other what happened.
 */
export interface Tabs {
  /** Runs `work` once no other tab holds the Sync lock, holding it meanwhile. */
  exclusive<T>(work: () => Promise<T>): Promise<T>;
  /** Tells the other tabs (not this one). */
  post(message: TabMessage): void;
  /** Calls `listener` with what other tabs tell. */
  listen(listener: (message: TabMessage) => void): void;
}

/** Web Locks and a BroadcastChannel; a tab alone where the browser has neither. */
export function browserTabs(): Tabs {
  const channel =
    typeof BroadcastChannel === "undefined" ? undefined : new BroadcastChannel("sync");
  const locks = typeof navigator === "undefined" ? undefined : navigator.locks;
  return {
    exclusive: (work) => (locks ? locks.request("sync", work) : work()),
    post: (message) => channel?.postMessage(message),
    listen: (listener) => {
      if (channel) channel.onmessage = (event) => listener(event.data);
    },
  };
}
