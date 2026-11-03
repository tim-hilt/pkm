// The Elements the fixtures are generated for. Each scene becomes one
// `<name>.excalidraw` source file plus the `<name>.golden.json` Excalidraw
// output for it. Element ids describe the Element, and seeds are derived from
// the ids, so adding a case never changes the fixtures of the others.

const FILL_STYLES = ["hachure", "cross-hatch", "zigzag", "solid"];
const STROKE_STYLES = ["solid", "dashed", "dotted"];
const ROUGHNESSES = [0, 1, 2];
const CLOSED_SHAPES = ["rectangle", "diamond", "ellipse"];

const ARROWHEADS = [
  "arrow", "bar", "circle", "circle_outline", "triangle", "triangle_outline",
  "diamond", "diamond_outline", "cardinality_one", "cardinality_many",
  "cardinality_one_or_many", "cardinality_exactly_one", "cardinality_zero_or_one",
  "cardinality_zero_or_many",
];

// Excalidraw's roundness types (ROUNDNESS in packages/common/src/constants.ts).
const ADAPTIVE_RADIUS = { type: 3 }; // rectangles
const PROPORTIONAL_RADIUS = { type: 2 }; // diamonds and linear elements

const PINK = "#ffc9c9";

/**
 * The scenes to generate. `upstream.newElement(type, opts)` builds each
 * Element with Excalidraw's own constructor.
 */
export function scenes(upstream) {
  return [
    { name: "shapes", elements: shapes(upstream) },
    { name: "fills", elements: fills(upstream) },
    { name: "lines-arrows", elements: linesAndArrows(upstream) },
    { name: "arrowhead-turns", elements: arrowheadTurns(upstream) },
    { name: "freedraw", elements: freedraw(upstream) },
    // Excalidraw's own layout is the expected output: the file is its own golden.
    { name: "text", elements: texts(upstream), golden: false },
  ];
}

// --- shapes: roughness x stroke style x roundness --------------------------

function shapes(upstream) {
  const place = new Placer(9, 160, 120);
  const make = (type, id, opts) =>
    element(upstream, place, type, id, { ...closedSize(type), ...opts });

  const elements = [];
  for (const type of CLOSED_SHAPES) {
    for (const roughness of ROUGHNESSES) {
      for (const strokeStyle of STROKE_STYLES) {
        for (const round of type === "ellipse" ? [false] : [false, true]) {
          const id = `${type}-r${roughness}-${strokeStyle}-${round ? "round" : "sharp"}`;
          elements.push(make(type, id, { roughness, strokeStyle, roundness: round ? roundnessOf(type) : null }));
        }
      }
    }
  }
  // Small Elements get their roughness reduced by Excalidraw (adjustRoughness).
  for (const type of CLOSED_SHAPES) {
    elements.push(make(type, `${type}-tiny-r2`, { roughness: 2, width: 8, height: 8 }));
    elements.push(make(type, `${type}-narrow-r2`, { roughness: 2, width: 60, height: 12 }));
  }
  return elements;
}

// --- fills: every fill style on every closed shape -------------------------

function fills(upstream) {
  const place = new Placer(6, 160, 120);
  const make = (type, id, opts) =>
    element(upstream, place, type, id, { ...closedSize(type), backgroundColor: PINK, ...opts });

  const elements = [];
  for (const type of CLOSED_SHAPES) {
    for (const fillStyle of FILL_STYLES) {
      for (const roughness of ROUGHNESSES) {
        elements.push(make(type, `${type}-${fillStyle}-r${roughness}`, { fillStyle, roughness }));
      }
    }
  }
  for (const fillStyle of FILL_STYLES) {
    elements.push(make("rectangle", `rectangle-round-${fillStyle}`, { fillStyle, roundness: ADAPTIVE_RADIUS }));
    elements.push(make("diamond", `diamond-round-${fillStyle}`, { fillStyle, roundness: PROPORTIONAL_RADIUS }));
  }
  // Excalidraw derives the hachure gap and fill weight from the stroke width.
  for (const strokeWidth of [1, 4]) {
    for (const fillStyle of ["hachure", "cross-hatch", "zigzag"]) {
      elements.push(make("rectangle", `rectangle-${fillStyle}-w${strokeWidth}`, { fillStyle, strokeWidth }));
    }
  }
  return elements;
}

// --- lines and arrows -------------------------------------------------------

function linesAndArrows(upstream) {
  const place = new Placer(5, 220, 140);
  const make = (type, id, points, opts = {}) => {
    const [width, height] = extent(points);
    const el = element(upstream, place, type, id, { points, width, height, ...opts });
    return el;
  };

  const straight = [[0, 0], [150, 40]];
  const multi = [[0, 0], [60, 50], [120, 0], [180, 60]];
  const loop = [[0, 0], [100, 0], [100, 80], [0, 80], [0, 0]];

  const elements = [
    make("line", "line-straight", straight),
    make("line", "line-straight-r0", straight, { roughness: 0 }),
    make("line", "line-straight-r2", straight, { roughness: 2 }),
    make("line", "line-dashed", straight, { strokeStyle: "dashed" }),
    make("line", "line-dotted", straight, { strokeStyle: "dotted" }),
    make("line", "line-multi", multi),
    make("line", "line-curved", multi, { roundness: PROPORTIONAL_RADIUS }),
    make("line", "line-curved-two-points", straight, { roundness: PROPORTIONAL_RADIUS }),
    make("line", "line-loop-filled", loop, { backgroundColor: PINK, fillStyle: "hachure", polygon: true }),
    make("line", "line-loop-solid", loop, { backgroundColor: PINK, fillStyle: "solid", polygon: true }),
    make("arrow", "arrow-no-heads", straight, { endArrowhead: null }),
    make("arrow", "arrow-straight", straight, { endArrowhead: "arrow" }),
    make("arrow", "arrow-straight-r0", straight, { endArrowhead: "arrow", roughness: 0 }),
    make("arrow", "arrow-straight-r2", straight, { endArrowhead: "arrow", roughness: 2 }),
    make("arrow", "arrow-dashed", straight, { endArrowhead: "arrow", strokeStyle: "dashed" }),
    make("arrow", "arrow-dotted", straight, { endArrowhead: "arrow", strokeStyle: "dotted" }),
    make("arrow", "arrow-thick", straight, { endArrowhead: "arrow", strokeWidth: 4 }),
    make("arrow", "arrow-short", [[0, 0], [24, 8]], { endArrowhead: "arrow" }),
    make("arrow", "arrow-multi", multi, { endArrowhead: "arrow" }),
    make("arrow", "arrow-curved", multi, { endArrowhead: "arrow", roundness: PROPORTIONAL_RADIUS }),
    make("arrow", "arrow-both", straight, { startArrowhead: "triangle", endArrowhead: "triangle_outline" }),
  ];
  for (const head of ARROWHEADS) {
    elements.push(make("arrow", `arrow-end-${head}`, straight, { endArrowhead: head }));
    elements.push(make("arrow", `arrow-start-${head}`, straight, { startArrowhead: head, endArrowhead: null }));
  }
  // Dotted and thick strokes size and space arrowheads differently.
  for (const head of ["arrow", "circle", "triangle_outline", "cardinality_zero_or_many"]) {
    elements.push(make("arrow", `arrow-dotted-end-${head}`, straight, { endArrowhead: head, strokeStyle: "dotted" }));
    elements.push(make("arrow", `arrow-thick-end-${head}`, straight, { endArrowhead: head, strokeWidth: 4 }));
  }
  // Curved loops are filled along their curve, not along their corners.
  const curvedLoop = [[0, 0], [60, -20], [100, 30], [50, 80], [0, 50], [0, 0]];
  for (const fillStyle of ["hachure", "solid"]) {
    elements.push(make("line", `line-curved-loop-${fillStyle}`, curvedLoop, {
      backgroundColor: PINK, fillStyle, roundness: PROPORTIONAL_RADIUS,
    }));
  }
  return elements;
}

// --- arrowheads on short, turning last segments ------------------------------

function arrowheadTurns(upstream) {
  const place = new Placer(6, 220, 140);
  const make = (id, points, opts) => {
    const [width, height] = extent(points);
    return element(upstream, place, "arrow", id, { points, width, height, ...opts });
  };

  const elements = [];
  // Arrowheads shrink on a short last segment and keep clear of the segment
  // before it, which turns away; start and end alike, sharp and curved.
  const turnAtEnd = [[0, 0], [100, 0], [108, 14]];
  const turnAtStart = [[0, 0], [8, 14], [108, 14]];
  const curvedTurn = [[0, 0], [60, 40], [100, 10], [110, 20]];
  for (const head of ARROWHEADS) {
    elements.push(make(`arrow-turn-end-${head}`, turnAtEnd, { endArrowhead: head }));
    elements.push(make(`arrow-turn-start-${head}`, turnAtStart, { startArrowhead: head, endArrowhead: null }));
  }
  for (const head of ["arrow", "bar", "circle", "triangle", "diamond", "cardinality_one", "cardinality_zero_or_many"]) {
    elements.push(make(`arrow-turn-thick-end-${head}`, turnAtEnd, { endArrowhead: head, strokeWidth: 4 }));
    elements.push(make(`arrow-turn-curved-end-${head}`, curvedTurn, { endArrowhead: head, roundness: PROPORTIONAL_RADIUS }));
  }
  return elements;
}

// --- freedraw: simulated and real pressure ----------------------------------

function freedraw(upstream) {
  const place = new Placer(4, 220, 140);
  const make = (id, points, opts = {}) => {
    const [width, height] = extent(points);
    return element(upstream, place, "freedraw", id, { points, width, height, ...opts });
  };

  // A wavy stroke sampled every ~6px, as a finger or mouse would produce.
  const wave = Array.from({ length: 24 }, (_, i) => [round2(i * 6), round2(Math.sin(i / 3) * 18)]);
  // The same stroke from a stylus: pressure rises and falls along it.
  const pressures = wave.map((_, i) => round2(0.25 + 0.6 * Math.sin((i / 23) * Math.PI)));
  const circle = Array.from({ length: 25 }, (_, i) => {
    const a = (i / 24) * 2 * Math.PI;
    return [round2(40 + 40 * Math.cos(a) - 40), round2(40 * Math.sin(a))];
  });
  const circlePressures = circle.map((_, i) => round2(0.4 + 0.3 * Math.cos(i / 4)));

  return [
    make("freedraw-simulated-wave", wave, { simulatePressure: true, pressures: [] }),
    make("freedraw-real-wave", wave, { simulatePressure: false, pressures }),
    make("freedraw-simulated-thick", wave, { simulatePressure: true, pressures: [], strokeWidth: 4 }),
    make("freedraw-real-thick", wave, { simulatePressure: false, pressures, strokeWidth: 4 }),
    make("freedraw-simulated-precise", wave, {
      simulatePressure: true, pressures: [], strokeOptions: { variability: "variable", streamline: 0.2 },
    }),
    make("freedraw-simulated-dot", [[0, 0]], { simulatePressure: true, pressures: [] }),
    make("freedraw-real-dot", [[0, 0]], { simulatePressure: false, pressures: [0.5] }),
    make("freedraw-simulated-short", [[0, 0], [2, 1]], { simulatePressure: true, pressures: [] }),
    make("freedraw-simulated-loop-filled", circle, { simulatePressure: true, pressures: [], backgroundColor: PINK, fillStyle: "hachure" }),
    make("freedraw-real-loop-filled", circle, { simulatePressure: false, pressures: circlePressures, backgroundColor: PINK, fillStyle: "solid" }),
  ];
}

// --- text: alignment, wrapping, labels in containers -------------------------

const LOREM = "The quick brown fox jumps over the lazy dog";

function texts(upstream) {
  const place = new Placer(4, 260, 200);
  const elements = [];
  /**
   * Adds the text `id`; `container` is `[type, extra Element options]` for a
   * label. Both take the next free cell and the next fractional indexes.
   */
  const add = (id, text, { container, ...opts } = {}) => {
    const cell = place.next();
    const base = { x: cell.x, y: cell.y, created: 1730380000000 };
    const box =
      container &&
      element(upstream, { next: () => cell }, container[0], `${id}-box`, {
        ...closedSize(container[0]),
        width: 160,
        height: 90,
        ...container[1],
      });
    const [label, grown] = upstream.newText({
      text,
      id,
      seed: seedFor(id),
      ...base,
      ...opts,
      container: box,
    });
    for (const el of grown ? [grown, label] : [label]) {
      el.updated = 1730380000000;
      el.index = fractionalIndex(elements.length);
      elements.push(el);
    }
  };

  // Free text: each alignment, several lines and sizes, other font families.
  for (const textAlign of ["left", "center", "right"]) {
    add(`text-${textAlign}`, "Hello\nWorld, a longer line\n\nlast", { textAlign });
  }
  add("text-large", "Big", { fontSize: 36 });
  add("text-virgil", "Virgil family", { fontFamily: 1 });
  add("text-helvetica", "Helvetica family", { fontFamily: 2 });
  add("text-red", "Red and translucent", { strokeColor: "#e03131", opacity: 60 });
  add("text-rotated", "Turned about\nits middle", { angle: 0.5 });

  // Fixed width: wraps to its own width.
  add("text-fixed", LOREM, { width: 140 });
  add("text-fixed-right", LOREM, { width: 140, textAlign: "right" });

  // Wrapping rules: hyphens, spaces, words longer than the line, CJK.
  add("text-hyphens", "well-known state-of-the-art long-winded", { width: 120 });
  add("text-spaces", "a    b    c    d    e    f    g", { width: 100 });
  add("text-long-word", "Supercalifragilisticexpialidocious!", { width: 100 });
  add("text-cjk", "日本語のテキストは文字ごとに折り返されます", { width: 100 });
  add("text-trailing", "ends with spaces   \nnext", { width: 120 });

  // Labels: every container shape, alignment and rotation.
  add("label-rectangle", LOREM, { container: ["rectangle", {}] });
  add("label-ellipse", LOREM, { container: ["ellipse", { width: 200, height: 120 }] });
  add("label-diamond", LOREM, { container: ["diamond", { width: 260, height: 160 }] });
  add("label-top-left", LOREM, { container: ["rectangle", {}], verticalAlign: "top", textAlign: "left" });
  add("label-bottom-right", LOREM, { container: ["rectangle", {}], verticalAlign: "bottom", textAlign: "right" });
  add("label-short", "Hi", { container: ["rectangle", {}] });
  add("label-rotated", LOREM, { container: ["rectangle", { angle: 0.6 }] });
  add("label-grows", `${LOREM}. ${LOREM}`, { container: ["rectangle", { height: 40 }] });
  add("label-big-font", "Big label", { container: ["ellipse", {}], fontSize: 28 });
  add("label-multiline", "one\ntwo\nthree", { container: ["diamond", { width: 200, height: 140 }] });
  return elements;
}

// --- helpers ----------------------------------------------------------------

/** Lays Elements out on a grid, left to right, so the scene is viewable. */
class Placer {
  constructor(columns, cellWidth, cellHeight) {
    Object.assign(this, { columns, cellWidth, cellHeight, count: 0 });
  }
  next() {
    const i = this.count++;
    return { x: (i % this.columns) * this.cellWidth, y: Math.floor(i / this.columns) * this.cellHeight, index: i };
  }
}

/** Builds one Element; `id` fixes its id and (through a hash) its seed. */
function element(upstream, place, type, id, opts) {
  const { x, y, index } = place.next();
  const el = upstream.newElement(type, {
    id,
    x,
    y,
    seed: seedFor(id),
    index: fractionalIndex(index),
    created: 1730380000000,
    ...opts,
  });
  el.updated = 1730380000000;
  return el;
}

function closedSize(type) {
  return { width: 100, height: type === "diamond" ? 80 : 60 };
}

function roundnessOf(type) {
  return type === "rectangle" ? ADAPTIVE_RADIUS : PROPORTIONAL_RADIUS;
}

/** Bounding-box size of local points that all start at the Element's origin. */
function extent(points) {
  const xs = points.map(([x]) => x);
  const ys = points.map(([, y]) => y);
  return [Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys)];
}

/** A stable positive 31-bit seed (FNV-1a); never 0, which rough.js randomizes. */
function seedFor(id) {
  let hash = 0x811c9dc5;
  for (const ch of id) {
    hash = Math.imul(hash ^ ch.charCodeAt(0), 0x01000193) >>> 0;
  }
  return (hash & 0x7fffffff) || 1;
}

/** Excalidraw's fractional index for the i-th Element: "a0".."aZ", then "b00".. */
function fractionalIndex(i) {
  const digits = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
  if (i < digits.length) return `a${digits[i]}`;
  throw new Error("scene too large for the simple index scheme");
}

function round2(n) {
  return Math.round(n * 100) / 100;
}
