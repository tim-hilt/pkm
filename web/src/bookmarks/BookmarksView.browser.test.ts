import { userEvent } from "vitest/browser";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { MemoryStorage } from "../storage/MemoryStorage";
import { BookmarkFiles } from "./BookmarkFiles";
import { BookmarksModel } from "./BookmarksModel";
import { BookmarksView } from "./BookmarksView";
import { rangeOf } from "./ArticleOffsets";
import { PageDownloader } from "./PageDownloader";

beforeAll(async () => {
  await loadCore(fetch(wasmUrl));
});

const views: BookmarksView[] = [];
afterEach(() => views.splice(0).forEach((view) => view.element.remove()));

const open = async (send: (url: string) => Promise<Response>) => {
  const model = new BookmarksModel(
    await BookmarkFiles.open(new MemoryStorage()),
    new PageDownloader(send),
    () => [],
  );
  const places = {
    opened: [] as string[],
    openDetails: (id: string) => places.opened.push(id),
    closeDetails() {},
    immersive: (on: boolean) => places.immersions.push(on),
    immersions: [] as boolean[],
  };
  const view = new BookmarksView(model, places);
  document.body.append(view.element);
  views.push(view);
  return { model, view, places };
};

const wait = async (done: () => boolean) => {
  for (let tries = 0; tries < 100 && !done(); tries++) await new Promise((r) => setTimeout(r, 20));
};

describe("BookmarksView", () => {
  it("says how to Save a first page while empty", async () => {
    const { view } = await open(() => Promise.reject(new Error("no download expected")));
    expect(view.element.textContent).toContain("No Bookmarks");
  });

  it("shows why a page was refused and Saves its Address only", async () => {
    const { model, view } = await open(
      async () => new Response("host is private, loopback or LAN\n", { status: 403 }),
    );
    view.element.querySelector<HTMLElement>("[aria-label='Add Bookmark']")!.click();
    await userEvent.keyboard("http://192.168.1.2/admin{Enter}");
    await wait(
      () => view.element.querySelector("dialog")?.textContent?.includes("address only") === true,
    );
    expect(view.element.querySelector("dialog")!.textContent).toContain(
      "Saved with its address only: Host is private, loopback or LAN.",
    );
    await userEvent.keyboard("{Enter}");
    await wait(() => model.all.length > 0);
    expect(model.all.map((b) => [b.address, b.hasArticle])).toEqual([
      ["http://192.168.1.2/admin", false],
    ]);
    expect(view.element.querySelector(".task-row")?.textContent).toContain(
      "http://192.168.1.2/admin",
    );
  });
});

const paragraphs = Array.from(
  { length: 40 },
  (_, n) =>
    `<p>Paragraph ${n} says the fox is worth reading about. ${"Filler words. ".repeat(30)}</p>`,
).join("");
const ARTICLE = `<html><head><title>On foxes</title></head><body><article><h1>On foxes</h1>${paragraphs}</article></body></html>`;

/** The Reader open on a long Article served as a page. */
const reading = async () => {
  const opened = await open(
    async () => new Response(ARTICLE, { headers: { "Content-Type": "text/html" } }),
  );
  const prepared = await opened.model.prepare("https://blog.example/foxes");
  if (prepared.kind !== "draft") throw new Error("expected a draft");
  const bookmark = await opened.model.save(prepared.draft, "", [], true);
  opened.model.openBookmark(bookmark, () => {});
  const query = <T extends Element>(selector: string) =>
    opened.view.element.querySelector<T>(selector)!;
  return { ...opened, bookmark, query, article: query<HTMLElement>(".bookmark-article") };
};

const select = (article: HTMLElement, start: number, end: number) => {
  getSelection()!.removeAllRanges();
  getSelection()!.addRange(rangeOf(article, start, end));
};

describe("the Reader", () => {
  it("Highlights selected text, across paragraphs, in the Default Highlight Color", async () => {
    const { model, bookmark, query, article } = await reading();
    const text = article.textContent!;
    const start = text.indexOf("fox is worth", text.indexOf("Paragraph 1 "));
    const end = text.indexOf("Paragraph 3 ");
    select(article, start, end);
    await wait(() => !query<HTMLElement>(".reader-selection").hidden);
    [...query(".reader-selection").querySelectorAll("button")]
      .find((b) => b.textContent === "Highlight")!
      .click();
    await wait(() => model.markup(bookmark.id).highlights.length > 0);

    const [highlight] = model.markup(bookmark.id).highlights;
    expect([highlight!.start, highlight!.end, highlight!.rgb]).toEqual([start, end, 0xffeb3c]);
    expect(highlight!.quotedText).toBe(model.article(bookmark.id)!.slice(start, end));
    expect(CSS.highlights.get("reader-ink-ffeb3c")?.size).toBe(1);
  });

  it("recolors and deletes a Highlight from its popover", async () => {
    const { model, bookmark, query, article } = await reading();
    const start = article.textContent!.indexOf("Paragraph 0 ");
    model.addHighlight(bookmark.id, start, start + 11, "Paragraph 0", 0xffeb3c);
    // Opened again so that the Reader draws the Highlight it finds.
    model.closeReader();
    model.openBookmark(bookmark, () => {});
    const text = query<HTMLElement>(".bookmark-article");
    const rect = rangeOf(text, start, start + 11).getBoundingClientRect();
    await userEvent.click(text, {
      position: {
        x: rect.left - text.getBoundingClientRect().left + 5,
        y: rect.top - text.getBoundingClientRect().top + 5,
      },
    });
    await wait(() => !query<HTMLElement>(".reader-popover").hidden);
    query<HTMLElement>("[aria-label=Blue]").click();
    await wait(() => model.markup(bookmark.id).highlights[0]?.rgb === 0x66b2ff);
    await userEvent.type(query("textarea"), "Why it matters");
    [...query(".reader-popover").querySelectorAll("button")]
      .find((b) => b.textContent === "Delete")!
      .click();
    await wait(() => model.markup(bookmark.id).highlights.length === 0);
    expect(query(".reader-note")).toBeNull();
  });

  it("finds text and Jumps between matches", async () => {
    const { query, article } = await reading();
    [...query(".bookmarks-reader .bookmarks-bar").querySelectorAll("button")]
      .find((b) => b.textContent === "Find")!
      .click();
    await userEvent.keyboard("worth reading");
    await wait(() => query(".reader-find-count").textContent === "1 of 40");
    expect(CSS.highlights.get("reader-found")?.size).toBe(40);
    expect(CSS.highlights.get("reader-found-current")?.size).toBe(1);

    await userEvent.keyboard("{Enter}");
    expect(query(".reader-find-count").textContent).toBe("2 of 40");
    const current = [...CSS.highlights.get("reader-found-current")!][0] as Range;
    const scroll = query<HTMLElement>(".bookmarks-reader .tasks-scroll");
    const top = current.getBoundingClientRect().top - scroll.getBoundingClientRect().top;
    expect(top).toBeGreaterThan(0);
    expect(top).toBeLessThan(scroll.clientHeight);

    await userEvent.keyboard("{Shift>}{Enter}{/Shift}{Shift>}{Enter}{/Shift}");
    expect(query(".reader-find-count").textContent).toBe("40 of 40");
    expect(article.textContent).toContain("worth reading");
    await userEvent.keyboard("{Escape}");
    expect(CSS.highlights.get("reader-found")).toBeUndefined();
  });

  it("keeps Reading Progress, and reopens where it was left", async () => {
    const { model, bookmark, query } = await reading();
    const scroll = query<HTMLElement>(".bookmarks-reader .tasks-scroll");
    // The page's stylesheet is not loaded: give the scrolling area its height.
    Object.assign(scroll.style, { height: "400px", overflow: "auto" });
    scroll.dispatchEvent(new Event("wheel"));
    scroll.scrollTop = 1500;
    await wait(() => model.bookmark(bookmark.id)!.offset !== undefined);
    expect(model.bookmark(bookmark.id)!.offset).toBeGreaterThan(1000);

    model.closeReader();
    model.openBookmark(model.bookmark(bookmark.id)!, () => {});
    const reopened = query<HTMLElement>(".bookmarks-reader .tasks-scroll");
    Object.assign(reopened.style, { height: "400px", overflow: "auto" });
    await wait(() => reopened.scrollTop > 0);
    expect(Math.abs(reopened.scrollTop - 1500)).toBeLessThan(60);
  });

  it("hides everything but the Article in Immersive Mode, and shows it again on Escape", async () => {
    const { places, query, article } = await reading();
    await userEvent.click(article);
    expect(places.immersions).toEqual([true]);
    expect(query(".bookmarks-reader").classList.contains("is-immersive")).toBe(true);
    await userEvent.keyboard("{Escape}");
    expect(places.immersions).toEqual([true, false]);
    expect(query(".bookmarks-reader").classList.contains("is-immersive")).toBe(false);
  });
});
