// Usage: [SCREEN=filters|filter] node scripts/screenshot.mjs [out.png] [url-path]
// SCREEN=filters shows the Filters list, SCREEN=filter one Filter's Tasks, over
// Tasks and Filters it first makes through the UI.
// Serves nothing itself: expects the dev server on $PORT (default 3000).
// Captures the page with Playwright's Chromium, WebGPU on.
import { chromium } from "playwright-core";

const out = process.argv[2] ?? "/tmp/wt-web.png";
const screen = process.env.SCREEN ?? "";
const path = process.argv[3] ?? (screen ? "/tasks" : "/drawings");
const port = process.env.PORT ?? "3000";
const width = Number(process.env.WIDTH ?? 1280);
const height = Number(process.env.HEIGHT ?? 800);

const browser = await chromium.launch({
  args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=metal"],
});
try {
  const page = await browser.newPage({ viewport: { width, height }, deviceScaleFactor: 2 });
  page.on("console", (message) => console.error(`[page] ${message.text()}`));
  page.on("pageerror", (error) => console.error(`[page error] ${error.message}`));
  await page.goto(`http://localhost:${port}${path}`);
  await page.waitForSelector(".shell, .unsupported");
  if (screen === "filters" || screen === "filter") await seedFilters(page, screen === "filter");
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
