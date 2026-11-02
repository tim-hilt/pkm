// Generates the golden fixtures in crates/core/tests/fixtures/golden.
// See README.md in this directory.
//
//   npm ci && node generate.mjs [path/to/excalidraw/checkout]
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { scenes } from "./cases.mjs";
import { loadUpstream } from "./upstream.mjs";

const here = dirname(fileURLToPath(import.meta.url));
const OUTPUT_DIR = resolve(here, "../../crates/core/tests/fixtures/golden");
const DEFAULT_CHECKOUT = resolve(here, "../../../../excalidraw/excalidraw");

const upstream = await loadUpstream(process.argv[2] ?? process.env.EXCALIDRAW_DIR ?? DEFAULT_CHECKOUT);

mkdirSync(OUTPUT_DIR, { recursive: true });
for (const { name, elements } of scenes(upstream)) {
  writeFileSync(join(OUTPUT_DIR, `${name}.excalidraw`), sceneJson(elements));
  writeFileSync(join(OUTPUT_DIR, `${name}.golden.json`), goldenJson(upstream, elements));
  console.log(`${name}: ${elements.length} elements`);
}

/** The source file, shaped and indented like what excalidraw.com saves. */
function sceneJson(elements) {
  const scene = {
    type: "excalidraw",
    version: 2,
    source: "https://excalidraw.com",
    elements,
    appState: { gridSize: null, viewBackgroundColor: "#ffffff" },
    files: {},
  };
  return JSON.stringify(scene, null, 2) + "\n";
}

/**
 * The expected output. Layout: one line per Element's shape, so that diffs of
 * regenerated fixtures stay readable without pretty-printing every number.
 */
function goldenJson(upstream, elements) {
  const entries = elements.map((element) => {
    const shapes = upstream.drawn(element).map((shape) => `    ${JSON.stringify(shape)}`);
    return `  ${JSON.stringify(element.id)}: [\n${shapes.join(",\n")}\n  ]`;
  });
  return (
    `{\n"format": 1,\n"upstream": ${JSON.stringify(upstream.version)},\n"elements": {\n` +
    entries.join(",\n") +
    "\n}\n}\n"
  );
}
