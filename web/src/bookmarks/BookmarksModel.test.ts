import { readFileSync } from "node:fs";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { loadCore } from "../core/Core";
import { MemoryStorage } from "../storage/MemoryStorage";
import { BookmarkFiles } from "./BookmarkFiles";
import { BookmarksModel, type Draft } from "./BookmarksModel";
import { PageDownloader } from "./PageDownloader";

const generated = new URL("../../generated/", import.meta.url);
beforeAll(async () => {
  await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);
const text = (long: string) => `<p>${long.repeat(40)}</p>`;
const PAGE = `<html><head><title>A Post</title><meta property="og:site_name" content="Example"></head>
<body><nav><a href="/">Home</a></nav><article><h1>A Post</h1>${text("Some words worth reading. ")}
<img src="/pic.png"></article></body></html>`;

/** The pages the script serves; a missing one is refused as the script refuses. */
function script(pages: Record<string, { type: string; body: string | Uint8Array }>) {
  const asked: string[] = [];
  const send = async (url: string) => {
    asked.push(url);
    const [path, query = ""] = url.split("?");
    const address = new URLSearchParams(query).get("url") ?? "";
    const page = pages[address];
    if (path !== "/cgi-bin/fetch" || !page) {
      return new Response("host is private, loopback or LAN\n", { status: 403 });
    }
    return new Response(page.body as BodyInit, { headers: { "Content-Type": page.type } });
  };
  return { send, asked };
}

let storage: MemoryStorage;
let served: ReturnType<typeof script>;
let pages: Parameters<typeof script>[0];
let model: BookmarksModel;
beforeEach(async () => {
  storage = new MemoryStorage();
  pages = {
    "https://example.com/post": { type: "text/html; charset=utf-8", body: PAGE },
    "https://example.com/pic.png": { type: "image/png", body: PNG },
  };
  served = script(pages);
  model = new BookmarksModel(
    await BookmarkFiles.open(storage),
    new PageDownloader(served.send),
    () => [],
  );
});

const draftOf = async (address: string): Promise<Draft> => {
  const prepared = await model.prepare(address);
  if (prepared.kind !== "draft") throw new Error(`expected a draft, got ${prepared.kind}`);
  return prepared.draft;
};

describe("Saving an Address", () => {
  it("downloads the page and its images through the script and Saves them", async () => {
    const draft = await draftOf("example.com/post#top");
    expect(served.asked).toEqual([
      "/cgi-bin/fetch?url=https%3A%2F%2Fexample.com%2Fpost",
      "/cgi-bin/fetch?url=https%3A%2F%2Fexample.com%2Fpic.png",
    ]);
    expect([draft.title, draft.readsLikeArticle, draft.failure]).toEqual([
      "A Post",
      true,
      undefined,
    ]);

    const saved = await model.save(draft, "", ["read"], true);
    await model.saving;
    expect(model.all.map((b) => [b.title, b.site, b.hasArticle, b.tags])).toEqual([
      ["A Post", "Example", true, ["read"]],
    ]);
    const article = model.article(saved.id)!;
    expect(article).toContain("Some words worth reading.");
    const image = /([a-z0-9]+\.png)/.exec(article)![1]!;
    expect(await storage.read(`Bookmarks/${image}`)).toEqual(PNG);
    expect((await storage.files("Bookmarks")).some((f) => f.name === `${saved.id}.md`)).toBe(true);
    // The new Tag is known to everything that reads the Tags file.
    expect(new TextDecoder().decode(await storage.read("Tags/tags.json"))).toContain("read");
  });

  it("Saves the Address only for a refused download, giving the reason", async () => {
    const draft = await draftOf("https://intranet.local/secret");
    expect(draft.failure).toBe("Host is private, loopback or LAN.");
    expect([draft.title, draft.markdown]).toEqual(["https://intranet.local/secret", ""]);

    const saved = await model.save(draft, "", [], false);
    expect([saved.title, saved.hasArticle]).toEqual(["https://intranet.local/secret", false]);
  });

  it("Saves the Address only when the script cannot be reached", async () => {
    const offline = new PageDownloader(() => Promise.reject(new TypeError("offline")));
    model = new BookmarksModel(await BookmarkFiles.open(storage), offline, () => []);
    expect((await draftOf("https://example.com/post")).failure).toBe(
      "The page could not be downloaded.",
    );
  });

  it("leaves out an image that fails and keeps the rest", async () => {
    delete pages["https://example.com/pic.png"];
    const saved = await model.save(await draftOf("https://example.com/post"), "", [], true);
    expect(model.article(saved.id)).not.toContain("pic");
    expect(model.article(saved.id)).toContain("Some words worth reading.");
  });

  it("offers the Bookmark that has the Address already, however it is written", async () => {
    const saved = await model.save(await draftOf("https://example.com/post"), "", [], false);
    served.asked.length = 0;
    const again = await model.prepare("https://EXAMPLE.com/post?utm_source=x#top");
    expect(again).toEqual({ kind: "existing", bookmark: saved });
    expect(served.asked).toEqual([]);
  });

  it("refuses what is no web address", async () => {
    expect(await model.prepare("ftp://example.com/file")).toEqual({ kind: "invalid" });
  });

  it("downloads no page for a PDF", async () => {
    const draft = await draftOf("https://example.com/paper.pdf");
    expect([draft.failure, served.asked]).toEqual([undefined, []]);
  });

  it("reports the progress of the downloads", async () => {
    const steps: [number, number][] = [];
    await model.prepare("https://example.com/post", (done, total) => steps.push([done, total]));
    expect(steps).toEqual([
      [1, 2],
      [2, 2],
    ]);
  });
});

describe("the list", () => {
  afterEach(() => vi.useRealTimers());

  const save = async (address: string, tags: string[] = []) => {
    pages[address] = { type: "text/html", body: `<title>${address}</title>${text("word ")}` };
    return model.save(await draftOf(address), "", tags, false);
  };

  it("lists the newest Saved first and narrows to a Tag", async () => {
    // Saved is to the second.
    vi.useFakeTimers({ toFake: ["Date"] });
    await save("https://a.example/", ["x"]);
    vi.setSystemTime(Date.now() + 2000);
    await save("https://b.example/", ["y"]);
    expect(model.shown.map((b) => b.address)).toEqual(["https://b.example/", "https://a.example/"]);
    expect(model.tagsInUse).toEqual(["x", "y"]);
    model.tagFilter = "x";
    expect(model.shown.map((b) => b.address)).toEqual(["https://a.example/"]);
  });

  it("finds a Bookmark by its words, with a snippet of the match", async () => {
    const saved = await save("https://a.example/");
    await save("https://b.example/");
    model.search("A.EXAMPLE");
    expect(model.results.map((hit) => hit.bookmark.id)).toEqual([saved.id]);
    const [hit] = model.results;
    expect(hit!.snippet.slice(hit!.matchStart, hit!.matchEnd).toLowerCase()).toBe("a.example");
  });

  it("renames, re-Tags, and Deletes with its files", async () => {
    const { id } = await save("https://a.example/", ["x"]);
    model.rename(id, "Mine");
    await model.setTags(id, ["z"]);
    expect(model.bookmark(id)).toMatchObject({ title: "Mine", tags: ["z"] });
    model.delete(id);
    await model.saving;
    expect(model.all).toEqual([]);
    expect((await storage.files("Bookmarks")).filter((f) => f.name.endsWith(".md"))).toEqual([]);
  });

  it("Adds an Article to a Bookmark without one and Drops it again", async () => {
    const saved = await model.save(await draftOf("https://example.com/post"), "", [], false);
    expect(saved.hasArticle).toBe(false);
    expect(await model.addArticle(saved.id)).toBe(true);
    expect(model.bookmark(saved.id)?.hasArticle).toBe(true);
    model.dropArticle(saved.id);
    await model.saving;
    expect(model.bookmark(saved.id)?.hasArticle).toBe(false);
    expect(model.article(saved.id)).toBeUndefined();
    const files = (await storage.files("Bookmarks")).map((f) => f.name);
    expect(files.some((name) => name.endsWith(".png"))).toBe(false);
  });

  it("does not Add an Article when the page can't be downloaded", async () => {
    const saved = await save("https://gone.example/");
    delete pages["https://gone.example/"];
    expect(await model.addArticle(saved.id)).toBe(false);
  });

  it("opens a Link to a Bookmark's Details; one that is gone leaves just the list", async () => {
    const { id } = await save("https://a.example/");
    model.openLinked(id);
    expect(model.detailsId).toBe(id);
    model.openLinked("nope");
    expect(model.detailsId).toBeUndefined();
  });

  it("opens an Article in the Reader and a Bookmark without one as a page", async () => {
    const plain = await save("https://a.example/");
    const withArticle = await model.save(await draftOf("https://example.com/post"), "", [], true);
    const opened: string[] = [];
    model.openBookmark(plain, (address) => opened.push(address));
    expect([model.openId, opened]).toEqual([undefined, ["https://a.example/"]]);
    model.openBookmark(withArticle, (address) => opened.push(address));
    expect(model.openId).toBe(withArticle.id);
  });
});

describe("after a restart", () => {
  it("lists what was Saved", async () => {
    const saved = await model.save(await draftOf("https://example.com/post"), "", ["a"], true);
    await model.saving;
    const reopened = new BookmarksModel(
      await BookmarkFiles.open(storage),
      new PageDownloader(served.send),
      () => [],
    );
    expect(reopened.all).toEqual([saved]);
    expect(reopened.article(saved.id)).toContain("Some words worth reading.");
  });
});
