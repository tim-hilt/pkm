import type { TabMessage, Tabs } from "./Tabs";

/** The tabs of one browser for tests: one lock, one channel, any number of `tab()`s. */
export class MemoryTabGroup {
  private queue: Promise<unknown> = Promise.resolve();
  private readonly listeners = new Set<(message: TabMessage) => void>();

  tab(): Tabs {
    let mine: ((message: TabMessage) => void) | undefined;
    return {
      exclusive: (work) => {
        const run = this.queue.then(work);
        this.queue = run.catch(() => undefined);
        return run;
      },
      post: (message) => {
        for (const listener of this.listeners) if (listener !== mine) listener(message);
      },
      listen: (listener) => {
        mine = listener;
        this.listeners.add(listener);
      },
    };
  }
}
