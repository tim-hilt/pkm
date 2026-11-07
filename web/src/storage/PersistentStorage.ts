/** The part of the browser's `StorageManager` that asks for persistence. */
export interface PersistenceRequester {
  persist(): Promise<boolean>;
}

/**
 * Returns a function that asks the browser for persistent storage the first
 * time it is called, so that it does not evict the Drawings when storage runs
 * short (the web app asks on the first save). The answer is the browser's to
 * give; a refusal or a browser without the API changes nothing.
 */
export function requestPersistenceOnFirstCall(
  storage: PersistenceRequester | undefined,
): () => void {
  let asked = false;
  return () => {
    if (asked) return;
    asked = true;
    storage?.persist?.().catch(() => {});
  };
}
