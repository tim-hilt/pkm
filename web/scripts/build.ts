// Production build: the bundle, the manifest and icons, and the service worker
// with the list of hashed assets to precache. Run with Bun (`bun run build`).
import { createHash } from "node:crypto";
import { cp, readFile, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import { buildMarker } from "../src/offline/OfflineCache";

const root = join(import.meta.dir, "..");
const dist = join(root, "dist");

await rm(dist, { recursive: true, force: true });

const app = await Bun.build({
  entrypoints: [join(root, "index.html")],
  outdir: dist,
  minify: true,
});
if (!app.success) throw new AggregateError(app.logs, "bundling the app failed");
await cp(join(root, "static"), dist, { recursive: true });

// Hashed files never change under their name; index.html is fetched
// network-first and the manifest is not needed offline.
const assets = app.outputs
  .map((output) => output.path.slice(dist.length + 1))
  .filter((path) => path !== "index.html");
const icons = await Array.fromAsync(new Bun.Glob("*.{png,svg}").scan(join(root, "static")));
const precached = [...assets, ...icons, "manifest.webmanifest"].sort();

// The version is the digest of everything precached; index.html refers to the
// hashed assets, so any change of the build changes the worker file and with
// it the cache.
const digest = createHash("sha256");
for (const file of precached) digest.update(await readFile(join(dist, file)));
const version = digest.digest("hex").slice(0, 16);

// The installable app's head tags, only in the build: the bundler would hash
// and rewrite these files, the manifest names them as they are.
const indexPath = join(dist, "index.html");
const pwaHead = [
  '<link rel="manifest" href="./manifest.webmanifest">',
  '<link rel="icon" href="./icon.svg" type="image/svg+xml">',
  '<link rel="apple-touch-icon" href="./apple-touch-icon.png">',
  '<meta name="theme-color" content="#ffffff">',
  buildMarker(version),
].join("");
const html = await readFile(indexPath, "utf8");
if (!html.includes("</head>")) throw new Error("index.html has no </head>");
await writeFile(indexPath, html.replace("</head>", `${pwaHead}</head>`));

const worker = await Bun.build({
  entrypoints: [join(root, "src/offline/serviceWorker.ts")],
  outdir: dist,
  naming: "sw.js",
  minify: true,
  define: { __BUILD__: JSON.stringify({ version, assets: precached }) },
});
if (!worker.success) throw new AggregateError(worker.logs, "bundling the worker failed");

console.log(`build ${version}: ${precached.length} files precached`);
