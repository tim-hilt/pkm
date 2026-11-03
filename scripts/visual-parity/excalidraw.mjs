// Exports every fixture as PNG from excalidraw.com, the same way a user would:
// open the file, then "Export image" → PNG at 2x with background. Writes
// ../../docs/visual-parity/<name>-excalidraw.png. Uses the system Chrome.
import { chromium } from "playwright";
import { readdirSync, mkdirSync } from "node:fs";

const here = new URL(".", import.meta.url).pathname;
const out = new URL("../../docs/visual-parity/", import.meta.url).pathname;
const url = process.env.EXCALIDRAW_URL ?? "https://excalidraw.com";
const only = process.argv.slice(2);

const browser = await chromium.launch({ channel: "chrome", headless: !process.env.HEADED });
const context = await browser.newContext({ viewport: { width: 1400, height: 900 }, deviceScaleFactor: 2, acceptDownloads: true });
// Without the File System Access API, Excalidraw falls back to <input type=file>
// for open and to a plain download for export, both of which Playwright sees.
await context.addInitScript(() => {
  delete window.showOpenFilePicker;
  delete window.showSaveFilePicker;
});
mkdirSync(out, { recursive: true });

for (const file of readdirSync(`${here}fixtures`).filter((f) => f.endsWith(".excalidraw"))) {
  const name = file.replace(/\.excalidraw$/, "");
  if (only.length && !only.includes(name)) continue;
  const page = await context.newPage();
  await page.goto(url);
  await page.evaluate(() => localStorage.clear());
  await page.reload();
  await page.locator("canvas.interactive").waitFor();
  // Excalifont must be loaded before export, or the text falls back.
  await page.evaluate(() => document.fonts.ready);

  // Open: Ctrl/Cmd+O shows a file chooser; "replace content" confirms.
  const chooser = page.waitForEvent("filechooser");
  await page.keyboard.press("ControlOrMeta+o");
  await (await chooser).setFiles(`${here}fixtures/${file}`);
  const replace = page.getByRole("button", { name: /replace/i });
  if (await replace.isVisible({ timeout: 1500 }).catch(() => false)) await replace.click();
  await page.waitForTimeout(1500);

  await page.keyboard.press("ControlOrMeta+Shift+e");
  const dialog = page.locator(".ImageExportModal");
  await dialog.waitFor();
  await dialog.locator(".RadioGroup__choice", { hasText: "2×" }).locator("input").check({ force: true });
  await page.waitForTimeout(500);
  const download = page.waitForEvent("download");
  await dialog.locator(".ImageExportModal__settings__buttons__button").first().click();
  await (await download).saveAs(`${out}${name}-excalidraw.png`);
  console.log(`${name}: exported`);
  await page.close();
}
await browser.close();
