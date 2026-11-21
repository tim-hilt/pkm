// Usage: [SCREEN=filters|filter|favorites|notes|bookmarks|article] [SCHEME=dark] node scripts/screenshot.mjs [out.png] [url-path]
// SCREEN=filters shows the Filters list, SCREEN=filter one Filter's Tasks, SCREEN=favorites the Filters list with many Favorites, over
// Tasks and Filters it first makes through the UI. SCREEN=notes shows Notes it makes so, the last one open.
// SCREEN=bookmarks shows Bookmarks it Saves through the UI from pages it serves in place of the server's fetch script, SCREEN=article the Reader over the first. SCHEME=dark emulates Dark mode.
// Serves nothing itself: expects the dev server on $PORT (default 3000).
// Captures the page with Playwright's Chromium, WebGPU on.
import { chromium } from "playwright-core";

const out = process.argv[2] ?? "/tmp/wt-web.png";
const screen = process.env.SCREEN ?? "";
const bookmarks = screen === "bookmarks" || screen === "article";
const path =
  process.argv[3] ??
  (screen === "notes" ? "/notes" : bookmarks ? "/bookmarks" : screen ? "/tasks" : "/drawings");
const port = process.env.PORT ?? "3000";
const width = Number(process.env.WIDTH ?? 1280);
const height = Number(process.env.HEIGHT ?? 800);

const pixel = Buffer.from(
  "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==",
  "base64",
);
const prose = (words) => `<p>${words.repeat(6)}</p>`;
const pages = {
  "https://blog.example/on-foxes": `<html><head><title>On foxes</title>
    <meta property="og:site_name" content="Fox Blog"><meta property="og:image" content="https://blog.example/fox.png"></head>
    <body><nav><a href="/">Home</a></nav><article><h1>On foxes</h1>
    ${prose("The quick brown fox, tired of running, sat down to write about it. ")}
    <h2>Habits</h2><ul><li>Hunts at <em>dusk</em></li><li>Digs <strong>dens</strong></li></ul>
    <blockquote><p>A fox is a wolf who sends flowers.</p></blockquote>
    ${prose("Second paragraph, with a <a href='https://example.com/more'>link</a> to read on. ")}
    <img src="https://blog.example/fox.png"><pre><code>fn main() {}</code></pre></article></body></html>`,
  "https://docs.example/start": `<title>Getting started</title>${prose("Install it, then run it. ")}`,
  "https://news.example/story": `<title>The story</title><meta property="og:site_name" content="Daily News">${prose("Something happened today. ")}`,
};

const browser = await chromium.launch({
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=metal"],
});
try {
  const page = await browser.newPage({
    viewport: { width, height },
    deviceScaleFactor: 2,
    colorScheme: process.env.SCHEME === "dark" ? "dark" : "light",
  });
  page.on("console", (message) => console.error(`[page] ${message.text()}`));
  page.on("pageerror", (error) => console.error(`[page error] ${error.message}`));
  await page.goto(`http://localhost:${port}${path}`);
  await page.waitForSelector(".shell, .unsupported");
  if (screen === "filters" || screen === "filter" || screen === "favorites") {
    await seedFilters(page, screen === "filter");
  }
  if (screen === "favorites") await seedFavorites(page);
  if (screen === "notes") await seedNotes(page);
  if (bookmarks) await seedBookmarks(page, screen === "article");
  await page.waitForTimeout(1500);
  await page.screenshot({ path: out });
  console.log(out);
} finally {
  await browser.close();
}

/** Makes Tasks and Filters through the UI, and opens the Filters list, or the first Filter. */
async function seedFilters(page, opensFilter) {
  for (const title of ["Review the draft @work ", "Book flights @home ", "Plan sprint @work "]) {
    await page.click(".tasks-new");
    await page.type(".tasks-quick-input", title);
    await page.click(".tasks-confirm");
    await page.keyboard.press("Escape");
  }
  await page.click(".tasks-switch-button:has-text('Filters')");
  for (const [title, query] of [
    ["Work", "@work"],
    ["Home or Garden", "@Home | @garden"],
    ["Broken", "@work &"],
  ]) {
    await page.click(".tasks-new");
    await page.fill("input[aria-label=Title]", title);
    await page.type("input[aria-label=Query]", query);
    if (query.endsWith("&")) {
      await page.screenshot({ path: out.replace(/\.png$/, "-dialog.png") });
      await page.keyboard.press("Escape");
    } else {
      await page.click(".tasks-confirm");
    }
  }
  if (opensFilter) await page.click(".task-open:has-text('Work')");
}

/** Adds Filters and marks them all as Favorites through the rows' menu, so that the bar overflows. */
async function seedFavorites(page) {
  for (const title of ["Reading list", "Errands", "Side project"]) {
    await page.click(".tasks-new");
    await page.fill("input[aria-label=Title]", title);
    await page.type("input[aria-label=Query]", "@home");
    await page.click(".tasks-confirm");
  }
  for (const title of ["Work", "Home or Garden", "Reading list", "Errands", "Side project"]) {
    await page.click(`.task-open:has-text('${title}')`, { button: "right" });
    await page.click("[role=menuitem]:has-text('Favorite')");
  }
}

/** Makes two Folders and writes Notes in the second through the UI; the last stays open. */
async function seedNotes(page) {
  for (const name of ["Personal", "Work"]) {
    await page.click(".notes-side .notes-bar button");
    await page.fill("input[aria-label=Name]", name);
    await page.keyboard.press("Enter");
    await page.waitForSelector(".tasks-dialog", { state: "detached" });
    // Beside the rows: back to the top level.
    if (name === "Personal")
      await page.click(".notes-side .tasks-scroll", { position: { x: 20, y: 300 } });
  }
  for (const [title, body] of [
    ["", "Buy oat milk\nand bread"],
    ["Reading list", "## Next up\n- `Rust` in **Action**\n> Read a little every day"],
  ]) {
    await page.click(".notes .tasks-new");
    await page.keyboard.type(title);
    await page.click(".notes-editor .tasks-description");
    await page.keyboard.type(body);
  }
}

/** Saves the pages through the UI, the server's fetch script answered by \`pages\`; the first one's Article is open for \`reading\`. */
async function seedBookmarks(page, reading) {
  await page.route("**/cgi-bin/fetch?*", (route) => {
    const address = new URL(route.request().url()).searchParams.get("url");
    if (address in pages) {
      return route.fulfill({ contentType: "text/html", body: pages[address] });
    }
    if (address?.endsWith("fox.png"))
      return route.fulfill({ contentType: "image/png", body: pixel });
    return route.fulfill({
      status: 403,
      contentType: "text/plain",
      body: "host is private, loopback or LAN\n",
    });
  });
  const saved = [
    ["https://news.example/story", ["news"], false],
    ["https://docs.example/start", ["docs", "read"], false],
    ["https://blog.example/on-foxes", ["animals"], true],
  ];
  for (const [address, tags, article] of saved) {
    await page.click("button[aria-label='Add Bookmark']");
    await page.fill("input[aria-label=Address]", address);
    await page.keyboard.press("Enter");
    await page.waitForSelector("input[aria-label=Title]");
    for (const tag of tags) {
      await page.click(".bookmarks dialog[open] button[aria-label='Pick Tags']");
      await page.fill(".tasks-picker input[type=search]", tag);
      await page.click(".tasks-picker .tasks-suggestion");
      await page.click(".tasks-picker .tasks-confirm");
    }
    if (article) await page.check(".bookmarks dialog[open] input[type=checkbox]");
    await page.click(".bookmarks dialog[open] .tasks-confirm");
    await page.waitForSelector(".bookmarks dialog", { state: "detached" });
    await page.waitForTimeout(1100); // Saved is to the second, and newest comes first.
  }
  if (reading) await page.click(".bookmark-open:has-text('On foxes')");
}
