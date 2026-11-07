// Usage: node scripts/screenshot.mjs [out.png] [url-path]
// Serves nothing itself: expects the dev server on $PORT (default 3000).
// Captures the page with Playwright's Chromium, WebGPU on.
import { chromium } from "playwright-core";

const out = process.argv[2] ?? "/tmp/wt-web.png";
const path = process.argv[3] ?? "/?sample";
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
  await page.waitForSelector("canvas, .unsupported");
  await page.waitForTimeout(1500);
  await page.screenshot({ path: out });
  console.log(out);
} finally {
  await browser.close();
}
