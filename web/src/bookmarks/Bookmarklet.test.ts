import { describe, expect, it } from "vitest";
import { type HandedPage, type SaveWindow, bookmarklet, listenForHandedPages } from "./Bookmarklet";

const message = { type: "pkm-save", address: "https://example.com/a", html: "<p>Hi</p>" };

function opened() {
  const events = new EventTarget();
  const sent: unknown[] = [];
  const opener = { postMessage: (m: unknown) => sent.push(m) };
  const win: SaveWindow = {
    opener,
    addEventListener: (type, listener) =>
      events.addEventListener(type, (e) => listener(e as MessageEvent)),
  };
  const pages: HandedPage[] = [];
  listenForHandedPages(win, (page) => pages.push(page));
  const post = (data: unknown, source: unknown = opener) =>
    events.dispatchEvent(Object.assign(new Event("message"), { data, source }));
  return { post, pages, sent };
}

describe("the Save route", () => {
  it("says it is ready, then accepts the bookmarklet's message", () => {
    const { post, pages, sent } = opened();
    expect(sent).toEqual(["pkm-ready"]);
    post(message);
    expect(pages).toEqual([{ address: message.address, html: message.html }]);
  });

  it("ignores other shapes", () => {
    const { post, pages } = opened();
    post("pkm-save");
    post(null);
    post({ ...message, type: "other" });
    post({ ...message, html: 1 });
    post({ ...message, extra: true });
    post({ type: "pkm-save", address: message.address });
    post({ ...message, address: "" });
    expect(pages).toEqual([]);
  });

  it("ignores messages from other windows", () => {
    const { post, pages } = opened();
    post(message, {});
    expect(pages).toEqual([]);
  });

  it("does nothing in a window nobody opened", () => {
    const win: SaveWindow = {
      opener: null,
      addEventListener: () => expect.unreachable(),
    };
    listenForHandedPages(win, () => expect.unreachable());
  });
});

describe("the bookmarklet", () => {
  it("is a javascript: address for the app's origin", () => {
    const source = bookmarklet("https://pkm.example");
    expect(source.startsWith("javascript:")).toBe(true);
    expect(source).toContain('"https://pkm.example"');
    expect(() => new Function(source.slice("javascript:".length))).not.toThrow();
  });
});
