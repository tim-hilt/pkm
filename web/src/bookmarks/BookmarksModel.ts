import {
  type Details,
  Images,
  extractDetails,
  isDocumentAddress,
  normaliseAddress,
  readArticle,
} from "../core/Core";
import { DescriptionImages } from "../tasks/DescriptionImages";
import type { Tag } from "../tasks/TasksModel";
import type { BookmarkFiles } from "./BookmarkFiles";
import { DownloadFailure, type PageDownloader } from "./PageDownloader";

/** A Bookmark as a row shows it: a plain copy, as the core's objects must be freed. */
export interface BookmarkRow {
  id: string;
  /** Normalised. */
  address: string;
  title: string;
  site: string;
  /** The file name of the Cover. */
  cover: string | undefined;
  excerpt: string;
  author: string;
  published: string;
  /** UTC ISO 8601. */
  saved: string;
  hasArticle: boolean;
  tags: string[];
  /** How much of the Article was read, 0 to 100; `undefined` if never. */
  percent: number | undefined;
}

/** A Bookmark that Search found, with where in its snippet. */
export interface BookmarkHit {
  bookmark: BookmarkRow;
  snippet: string;
  /** The match in `snippet`: `matchStart` up to `matchEnd`. */
  matchStart: number;
  matchEnd: number;
}

/** A page ready to Save: what the core made of the download, for the Save dialog to show. */
export interface Draft {
  /** Normalised. */
  address: string;
  /** The page's Title (the Address where the page couldn't be read). */
  title: string;
  /** The page's text, empty where there is none. */
  markdown: string;
  /** Whether the page reads like an article: whether to keep the text unless the user says otherwise. */
  readsLikeArticle: boolean;
  /** Why the page couldn't be read, if it couldn't; the Bookmark is then Saved with its Address only. */
  failure: string | undefined;
  /** What `save` takes from the core: opaque to callers, freed by `save` or `discard`. */
  readonly details: Details;
  /** The downloaded Cover and Article images by their address; one that failed is missing. */
  readonly images: Map<string, Uint8Array>;
}

/** What preparing an Address came to. */
export type Prepared =
  | { kind: "invalid" }
  /** Its Address is Saved already. */
  | { kind: "existing"; bookmark: BookmarkRow }
  | { kind: "draft"; draft: Draft };

/** How many images are downloaded at once, which bounds the memory they take. */
const parallelDownloads = 6;

/**
 * The Bookmarks Context's state: the list, newest Saved first, narrowed to a Tag if one is
 * chosen, from the core's store and saved to storage after each change. It downloads, since only
 * a client can, and otherwise forwards to the store; every Bookmarks rule lives in the core. A
 * port of the Swift `BookmarksModel`.
 */
export class BookmarksModel {
  /** Every Bookmark, newest Saved first. */
  all: BookmarkRow[] = [];
  /** The Bookmarks the list shows: those carrying `tagFilter`, or all. */
  shown: BookmarkRow[] = [];
  /** What Search looks for; empty when not searching. */
  searchText = "";
  /** The Bookmarks that match `searchText`, Title matches first; empty when not searching. */
  results: BookmarkHit[] = [];
  /** The Bookmark whose Article the Reader shows. */
  openId: string | undefined;
  /** The Bookmark whose Details are shown. */
  detailsId: string | undefined;
  /** Why the last save failed; none after one that worked. */
  failure: string | undefined;
  /** Shows the Covers and the Articles' images from the kept files. */
  readonly images: DescriptionImages;
  /** Called after the rows or what is open changed. */
  onChange: () => void = () => {};
  private tag: string | undefined;

  constructor(
    private readonly files: BookmarkFiles,
    private readonly downloader: PageDownloader,
    /** The known Tags with their Tag Colors. */
    readonly knownTags: () => Tag[],
  ) {
    this.images = new DescriptionImages({
      addImage: () => undefined,
      readImage: (name) => files.readImage(name),
    });
    this.reload();
  }

  private get store() {
    return this.files.store;
  }

  /** The Tag the list is narrowed to, if any. */
  get tagFilter(): string | undefined {
    return this.tag;
  }

  set tagFilter(tag: string | undefined) {
    this.tag = tag;
    this.changed();
  }

  /** The Tags some Bookmark carries, alphabetically: what the list can be narrowed to. */
  get tagsInUse(): string[] {
    return [...new Set(this.all.flatMap((bookmark) => bookmark.tags))].sort();
  }

  /** Settles when what was changed is saved. */
  get saving(): Promise<void> {
    return this.files.saving;
  }

  /** The Bookmark `id`, if there is one. */
  bookmark(id: string | undefined): BookmarkRow | undefined {
    return this.all.find((bookmark) => bookmark.id === id);
  }

  /** Searches for `text`; results follow it and later changes. */
  search(text: string): void {
    this.searchText = text;
    this.changed();
  }

  /** The address names a Bookmark: its Details show; one that is gone, or none, leaves them closed. */
  openLinked(id: string | undefined): void {
    const shown = this.bookmark(id) ? id : undefined;
    if (shown === this.detailsId) return;
    this.detailsId = shown;
    this.onChange();
  }

  /** The Save dialog's first step: the Address as typed (`https://` is assumed if it names no scheme), its page, Cover and the Article's images downloaded and read. An Address Saved already isn't downloaded. A download that fails, or none for a PDF or EPUB, leaves the Address alone to be Saved. `progress` is called with the downloads done and to do. */
  async prepare(
    typed: string,
    progress: (done: number, total: number) => void = () => {},
  ): Promise<Prepared> {
    const text = typed.trim();
    const address = normaliseAddress(text.includes("://") ? text : `https://${text}`);
    if (address === undefined) return { kind: "invalid" };
    const existing = this.bookmark(this.all.find((b) => b.address === address)?.id);
    if (existing) return { kind: "existing", bookmark: existing };
    return { kind: "draft", draft: await this.fetch(address, true, progress) };
  }

  /** The page at `address`, its Cover if `withCover` and the Article's images downloaded and read. */
  private async fetch(
    address: string,
    withCover: boolean,
    progress: (done: number, total: number) => void = () => {},
  ): Promise<Draft> {
    let html = "";
    let failure: string | undefined;
    if (!isDocumentAddress(address)) {
      try {
        html = await this.downloader.page(address);
      } catch (error) {
        failure = error instanceof DownloadFailure ? error.message : "The page could not be read.";
      }
    }
    const details = extractDetails(html, address);
    const article = readArticle(html, address);
    const wanted = [...article.images];
    if (withCover && details.cover && !wanted.includes(details.cover)) wanted.push(details.cover);
    const images = new Map<string, Uint8Array>();
    let done = 1;
    progress(done, wanted.length + 1);
    await inParallel(wanted, parallelDownloads, async (image) => {
      const bytes = await this.downloader.image(image);
      if (bytes) images.set(image, bytes);
      progress(++done, wanted.length + 1);
    });
    return {
      address,
      title: details.title,
      markdown: article.markdown,
      readsLikeArticle: article.readsLikeArticle,
      failure,
      details,
      images,
    };
  }

  /** Frees what a draft holds, for one that is not Saved. */
  discard(draft: Draft): void {
    draft.details.free();
  }

  /**
   * Saves the page under `title` (blank keeps the page's) and `tags`, with its Article and images
   * if `keepsArticle`; the Bookmark, or the one Saved before under the same Address, which keeps
   * its own Tags.
   */
  async save(
    draft: Draft,
    title: string,
    tags: string[],
    keepsArticle: boolean,
  ): Promise<BookmarkRow> {
    try {
      await this.files.refreshTags();
      const saved = withImages(draft.images, (images) =>
        this.store.save(
          draft.address,
          draft.details,
          title,
          tags,
          keepsArticle ? draft.markdown : undefined,
          images,
        ),
      );
      const row = plain(saved.bookmark);
      saved.free();
      this.changed();
      return row;
    } finally {
      this.discard(draft);
    }
  }

  /** Renames the Bookmark; a blank `title` changes nothing. */
  rename(id: string, title: string): void {
    this.store.rename(id, title).free();
    this.changed();
  }

  /** Gives the Bookmark exactly `tags`: those it lacks are added in order, the others removed. Tags unknown become known. */
  async setTags(id: string, tags: string[]): Promise<void> {
    const current = this.bookmark(id)?.tags ?? [];
    await this.files.refreshTags();
    for (const tag of current.filter((tag) => !tags.includes(tag))) {
      this.store.removeTag(id, tag).free();
    }
    for (const tag of tags.filter((tag) => !current.includes(tag)))
      this.store.addTag(id, tag).free();
    this.changed();
  }

  /** Takes `tag` off every Bookmark, as part of Delete a Tag. */
  removeTagEverywhere(tag: string): void {
    this.store.removeTagEverywhere(tag);
    if (this.tag === tag) this.tag = undefined;
    this.changed();
  }

  /** Adds the Article of the page as it is now to a Bookmark without one. False if the page can't be downloaded or has no text, and nothing changes. */
  async addArticle(id: string): Promise<boolean> {
    const bookmark = this.bookmark(id);
    if (!bookmark || bookmark.hasArticle) return false;
    const page = await this.fetch(bookmark.address, false);
    try {
      if (page.failure !== undefined || page.markdown === "") return false;
      withImages(page.images, (images) => this.store.addArticle(id, page.markdown, images).free());
    } finally {
      this.discard(page);
    }
    this.changed();
    return true;
  }

  /** Takes the Article, with its images, off the Bookmark, which closes its Reader. */
  dropArticle(id: string): void {
    if (this.openId === id) this.openId = undefined;
    this.store.dropArticle(id).free();
    this.changed();
  }

  /** Deletes the Bookmark with its Cover, Article and images. */
  delete(id: string): void {
    if (this.openId === id) this.openId = undefined;
    if (this.detailsId === id) this.detailsId = undefined;
    this.store.delete(id);
    this.changed();
  }

  /** The Markdown of the Bookmark's Article; `undefined` if it has none. */
  article(id: string): string | undefined {
    try {
      return this.store.readArticle(id);
    } catch {
      return undefined;
    }
  }

  /** Opens the Bookmark: its Article in the Reader, or else its page, which `open` shows. */
  openBookmark(bookmark: BookmarkRow, open: (address: string) => void): void {
    if (bookmark.hasArticle) {
      this.openId = bookmark.id;
      this.onChange();
    } else open(bookmark.address);
  }

  /** Closes the Reader. */
  closeReader(): void {
    this.openId = undefined;
    this.onChange();
  }

  /** Saves what changed, lists again, then tells the view. */
  private changed(): void {
    this.reload();
    this.files.save().then(
      () => this.setFailure(undefined),
      (error: unknown) => this.setFailure(error instanceof Error ? error.message : String(error)),
    );
    this.onChange();
  }

  private setFailure(failure: string | undefined): void {
    if (failure === this.failure) return;
    this.failure = failure;
    this.onChange();
  }

  private reload(): void {
    const { store } = this;
    this.all = store.bookmarks().map(plain);
    this.shown = this.tag === undefined ? this.all : store.bookmarksTagged(this.tag).map(plain);
    this.results =
      this.searchText.trim() === ""
        ? []
        : store.search(this.searchText).map((found) => {
            const hit = {
              bookmark: plain(found.bookmark),
              snippet: found.snippet,
              matchStart: found.matchedStart,
              matchEnd: found.matchedEnd,
            };
            found.free();
            return hit;
          });
  }
}

/** A plain copy of a Bookmark of the core, which is freed. */
function plain(bookmark: {
  id: string;
  address: string;
  title: string;
  site: string;
  cover?: string;
  excerpt: string;
  author: string;
  published: string;
  saved: string;
  hasArticle: boolean;
  tags: string[];
  percent?: number;
  free(): void;
}): BookmarkRow {
  const row = {
    id: bookmark.id,
    address: bookmark.address,
    title: bookmark.title,
    site: bookmark.site,
    cover: bookmark.cover,
    excerpt: bookmark.excerpt,
    author: bookmark.author,
    published: bookmark.published,
    saved: bookmark.saved,
    hasArticle: bookmark.hasArticle,
    tags: bookmark.tags,
    percent: bookmark.percent,
  };
  bookmark.free();
  return row;
}

/** Calls `use` with the downloaded images as the core takes them, which are freed after. */
function withImages<T>(images: Map<string, Uint8Array>, use: (images: Images) => T): T {
  const set = new Images();
  try {
    for (const [address, bytes] of images) set.add(address, bytes);
    return use(set);
  } finally {
    set.free();
  }
}

/** Runs `work` on every item, at most `limit` at a time. */
async function inParallel<T>(
  items: T[],
  limit: number,
  work: (item: T) => Promise<void>,
): Promise<void> {
  const pending = [...items];
  const worker = async () => {
    for (let item = pending.shift(); item !== undefined; item = pending.shift()) await work(item);
  };
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker));
}
