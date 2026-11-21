import { userEvent } from "vitest/browser";
import { afterEach, beforeAll, describe, expect, it } from "vitest";
import { loadCore } from "../core/Core";
import wasmUrl from "../../generated/pkm_web_bg.wasm?url";
import { MemoryStorage } from "../storage/MemoryStorage";
import { BookmarkFiles } from "./BookmarkFiles";
import { BookmarksModel } from "./BookmarksModel";
import { BookmarksView } from "./BookmarksView";
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
