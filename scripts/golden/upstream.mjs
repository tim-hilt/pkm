// Loads the pinned Excalidraw checkout's own element code and exposes the two
// things the generator needs: constructing Elements exactly as Excalidraw does
// and asking Excalidraw what it would draw for them.
//
// The checkout has no node_modules, so its TypeScript is bundled with esbuild
// and the handful of third-party packages it imports are resolved from this
// directory's node_modules (pinned in package.json to the checkout's versions).
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build } from "esbuild";

const here = dirname(fileURLToPath(import.meta.url));

/** The Excalidraw commit all fixtures are generated from. */
export const EXCALIDRAW_COMMIT = "8bcdca4526867433f99fdfcb55a063668db0c9dc";

/** Packages whose installed version must equal the one the checkout uses. */
const LIBRARIES = ["roughjs", "perfect-freehand"];

/** Canvas background Excalidraw falls back to; arrowheads are filled with it. */
const RENDER_CONFIG = {
  isExporting: true, // bypasses Excalidraw's shape cache
  canvasBackgroundColor: "#ffffff",
  embedsValidationStatus: null,
  theme: "light",
};

const ENTRY = `
  export { newElement, newLinearElement, newArrowElement, newFreeDrawElement } from "@excalidraw/element/newElement";
  export { ShapeCache, getFreedrawOutlinePoints } from "@excalidraw/element/shape";
  export { newTextElement } from "@excalidraw/element/newElement";
  export { wrapText } from "@excalidraw/element/textWrapping";
  export { measureText, setCustomTextMetricsProvider } from "@excalidraw/element/textMeasurements";
  export {
    computeBoundTextPosition,
    computeContainerDimensionForBoundText,
    getBoundTextMaxHeight,
    getBoundTextMaxWidth,
  } from "@excalidraw/element/textElement";
  export { getFontString } from "@excalidraw/common";
`;

/**
 * Loads the checkout at `checkoutDir` (throws unless it is the pinned commit
 * and uses the library versions installed here).
 *
 * Returns:
 *  - `version`: `{ excalidraw, roughjs, "perfect-freehand" }`, for fixture headers.
 *  - `newElement(type, opts)`: an Element built by Excalidraw's own constructor
 *    for `type` (rectangle, diamond, ellipse, line, arrow, freedraw), with the
 *    constructor's defaults for everything `opts` leaves out.
 *  - `drawn(element)`: what Excalidraw draws for `element`, in z-order, as
 *    `[{kind:"rough", shape, options, sets} | {kind:"outline", points}]`.
 */
export async function loadUpstream(checkoutDir) {
  const root = resolve(checkoutDir);
  const version = verifyPins(root);
  const upstream = await importBundle(await bundle(root));

  const constructors = {
    rectangle: upstream.newElement,
    diamond: upstream.newElement,
    ellipse: upstream.newElement,
    line: upstream.newLinearElement,
    arrow: upstream.newArrowElement,
    freedraw: upstream.newFreeDrawElement,
  };

  // Text is measured with the fixed character widths of the core's fake
  // `TextMeasurer`: every character is half as wide as the font is high.
  upstream.setCustomTextMetricsProvider({
    getLineWidth: (text, font) => Array.from(text).length * (parseFloat(font) / 2),
  });

  return {
    version,
    /**
     * A text Element as Excalidraw lays it out. With `container`, a label
     * wrapped to it and placed in it (growing the container if it must, as
     * Excalidraw does); with `width`, a text of fixed width wrapped to that;
     * else a text that is as wide as its longest line. Returns
     * `[text, container]`, where `container` is the changed container.
     */
    newText({ text, container, width, ...opts }) {
      const fontFamily = opts.fontFamily ?? 5;
      const fontSize = opts.fontSize ?? 20;
      const font = upstream.getFontString({ fontFamily, fontSize });
      const make = (wrapped, extra) =>
        upstream.newTextElement({ ...opts, text: wrapped, originalText: text, ...extra });

      if (container) {
        const maxWidth = upstream.getBoundTextMaxWidth(container, null);
        let label = make(upstream.wrapText(text, font, maxWidth), {
          containerId: container.id,
          verticalAlign: opts.verticalAlign ?? "middle",
          textAlign: opts.textAlign ?? "center",
          x: 0,
          y: 0,
        });
        const grown = { ...container, boundElements: [{ type: "text", id: label.id }] };
        if (label.height > upstream.getBoundTextMaxHeight(container, label)) {
          grown.height = upstream.computeContainerDimensionForBoundText(label.height, container.type);
        }
        label = { ...label, angle: grown.angle, ...upstream.computeBoundTextPosition(grown, label, new Map()) };
        return [label, grown];
      }
      if (width !== undefined) {
        const label = make(upstream.wrapText(text, font, width), { autoResize: false });
        const { height } = upstream.measureText(label.text, font, label.lineHeight);
        return [{ ...label, width, height }];
      }
      return [make(text, {})];
    },
    newElement(type, opts) {
      const construct = constructors[type];
      if (!construct) throw new Error(`unsupported element type ${type}`);
      return construct({ ...opts, type });
    },
    drawn(element) {
      const shape = upstream.ShapeCache.generateElementShape(element, RENDER_CONFIG);
      const drawables = (Array.isArray(shape) ? shape : [shape]).filter(
        // Excalidraw keeps a freedraw stroke as a rounded SVG path string;
        // the exact outline points are taken from the same function instead.
        (s) => typeof s !== "string",
      );
      const result = drawables.map(roughShape);
      if (element.type === "freedraw") {
        result.push({ kind: "outline", points: upstream.getFreedrawOutlinePoints(element) });
      }
      return result;
    },
  };
}

function roughShape({ shape, options, sets }) {
  // rough.js leaves its PRNG state on the options; it is not part of the output.
  const { randomizer, ...plain } = options;
  return { kind: "rough", shape, options: plain, sets };
}

/** Throws unless the checkout and installed libraries are the pinned ones. */
function verifyPins(root) {
  const head = execFileSync("git", ["-C", root, "rev-parse", "HEAD"], { encoding: "utf8" }).trim();
  if (head !== EXCALIDRAW_COMMIT) {
    throw new Error(`${root} is at ${head}; fixtures are pinned to ${EXCALIDRAW_COMMIT}`);
  }
  const wanted = JSON.parse(readFileSync(join(root, "packages/excalidraw/package.json"), "utf8"));
  const version = { excalidraw: head };
  for (const name of LIBRARIES) {
    const installed = JSON.parse(
      readFileSync(join(here, "node_modules", name, "package.json"), "utf8"),
    ).version;
    if (installed !== wanted.dependencies[name]) {
      throw new Error(
        `installed ${name}@${installed}, Excalidraw uses ${wanted.dependencies[name]}; update package.json`,
      );
    }
    version[name] = installed;
  }
  return version;
}

/** Bundles the checkout's element code; returns the text of an ES module. */
async function bundle(root) {
  const packages = join(root, "packages");
  const alias = Object.fromEntries(
    ["common", "element", "math", "utils", "laser-pointer", "fractional-indexing"].map((p) => [
      `@excalidraw/${p}`,
      join(packages, p, "src"),
    ]),
  );
  alias["@excalidraw/excalidraw"] = join(packages, "excalidraw");

  const { outputFiles } = await build({
    stdin: { contents: ENTRY, resolveDir: packages, sourcefile: "entry.ts", loader: "ts" },
    bundle: true,
    write: false,
    platform: "node",
    format: "esm",
    logLevel: "error",
    nodePaths: [join(here, "node_modules")],
    alias,
    // Excalidraw is built with Vite and reads its env from import.meta.env.
    define: { "import.meta.env": JSON.stringify({ MODE: "production", DEV: false, PROD: true }) },
  });
  return outputFiles[0].text;
}

/** Imports an ES module given as text (via a temp file, so stack traces stay readable). */
async function importBundle(code) {
  const dir = mkdtempSync(join(tmpdir(), "excalidraw-golden-"));
  try {
    const file = join(dir, "upstream.mjs");
    writeFileSync(file, code);
    return await import(pathToFileURL(file).href);
  } finally {
    rmSync(dir, { recursive: true });
  }
}
