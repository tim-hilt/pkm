import { describe, expect, it, vi } from "vitest";
import { requestPersistenceOnFirstCall } from "./PersistentStorage";

describe("asking for persistent storage", () => {
  it("happens on the first call only", () => {
    const persist = vi.fn(async () => true);
    const ask = requestPersistenceOnFirstCall({ persist });
    expect(persist).not.toHaveBeenCalled();
    ask();
    ask();
    ask();
    expect(persist).toHaveBeenCalledTimes(1);
  });

  it("survives a refusal, an error and a browser without the API", () => {
    expect(() => requestPersistenceOnFirstCall({ persist: async () => false })()).not.toThrow();
    expect(() =>
      requestPersistenceOnFirstCall({ persist: () => Promise.reject(new Error("no")) })(),
    ).not.toThrow();
    expect(() => requestPersistenceOnFirstCall(undefined)()).not.toThrow();
    expect(() => requestPersistenceOnFirstCall({} as never)()).not.toThrow();
  });
});
