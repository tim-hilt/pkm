// Writes the visual parity fixture Drawings into ./fixtures: one file per
// area, together covering every v1 Element type and style property.
// Deterministic (fixed ids and seeds), so rerunning leaves git clean.
import { mkdirSync, writeFileSync } from "node:fs";
import { deflateSync, crc32 } from "node:zlib";

const here = new URL(".", import.meta.url).pathname;

let counter = 0;
const base = (type, x, y, width, height, props = {}) => {
  counter += 1;
  return {
    id: `${type}-${counter}`,
    type,
    x,
    y,
    width,
    height,
    angle: 0,
    strokeColor: "#1e1e1e",
    backgroundColor: "transparent",
    fillStyle: "solid",
    strokeWidth: 2,
    strokeStyle: "solid",
    roughness: 1,
    opacity: 100,
    groupIds: [],
    frameId: null,
    roundness: null,
    seed: 1000 + counter * 7919,
    version: 1,
    versionNonce: counter,
    isDeleted: false,
    boundElements: null,
    updated: 1,
    link: null,
    locked: false,
    ...props,
  };
};

const linear = (type, x, y, points, props = {}) => {
  const xs = points.map((p) => p[0]);
  const ys = points.map((p) => p[1]);
  return base(type, x, y, Math.max(...xs) - Math.min(...xs), Math.max(...ys) - Math.min(...ys), {
    points,
    lastCommittedPoint: null,
    startBinding: null,
    endBinding: null,
    startArrowhead: null,
    endArrowhead: type === "arrow" ? "arrow" : null,
    roundness: { type: 2 },
    ...props,
  });
};

const text = (x, y, value, props = {}) => {
  const fontSize = props.fontSize ?? 20;
  const lines = value.split("\n");
  return base("text", x, y, Math.max(...lines.map((l) => l.length)) * fontSize * 0.55, lines.length * fontSize * 1.25, {
    text: value,
    originalText: value,
    fontSize,
    fontFamily: 5,
    textAlign: "left",
    verticalAlign: "top",
    containerId: null,
    lineHeight: 1.25,
    autoResize: true,
    ...props,
  });
};

const drawing = (elements, files = {}) => ({
  type: "excalidraw",
  version: 2,
  source: "https://excalidraw.com",
  elements,
  appState: { viewBackgroundColor: "#ffffff", gridSize: 20 },
  files,
});

// ── shapes: types × fill styles, edges, colours ─────────────────────────────
const shapes = () => {
  const out = [];
  const fills = ["hachure", "cross-hatch", "solid"];
  ["rectangle", "diamond", "ellipse"].forEach((type, row) => {
    fills.forEach((fillStyle, col) => {
      out.push(base(type, 40 + col * 180, 40 + row * 160, 140, 110, { fillStyle, backgroundColor: "#a5d8ff" }));
    });
    // unfilled, rounded edges (ellipses ignore roundness)
    out.push(base(type, 580, 40 + row * 160, 140, 110, { roundness: type === "ellipse" ? null : { type: 3 } }));
  });
  out.push(base("rectangle", 40, 520, 140, 110, { strokeColor: "#e03131", backgroundColor: "#ffc9c9", fillStyle: "hachure" }));
  out.push(base("ellipse", 220, 520, 140, 110, { strokeColor: "#2f9e44", backgroundColor: "#b2f2bb", fillStyle: "cross-hatch" }));
  out.push(base("diamond", 400, 520, 140, 110, { strokeColor: "#1971c2", backgroundColor: "#ffec99", fillStyle: "solid", roundness: { type: 2 } }));
  return out;
};

// ── strokes: width × style × sloppiness, opacity, rotation ──────────────────
const strokes = () => {
  const out = [];
  [1, 2, 4].forEach((strokeWidth, row) => {
    ["solid", "dashed", "dotted"].forEach((strokeStyle, col) => {
      out.push(base("rectangle", 40 + col * 160, 40 + row * 120, 120, 80, { strokeWidth, strokeStyle }));
    });
  });
  [0, 1, 2].forEach((roughness, col) => {
    out.push(base("rectangle", 40 + col * 160, 420, 120, 80, { roughness, backgroundColor: "#ffd8a8", fillStyle: "hachure" }));
  });
  [100, 60, 20].forEach((opacity, col) => {
    out.push(base("ellipse", 40 + col * 160, 540, 120, 80, { opacity, backgroundColor: "#d0bfff" }));
  });
  out.push(base("rectangle", 540, 60, 120, 80, { angle: Math.PI / 6, backgroundColor: "#ffc9c9" }));
  out.push(base("diamond", 540, 220, 120, 80, { angle: -Math.PI / 4 }));
  return out;
};

// ── lines and arrows: arrowheads, curved/sharp, closed, styles ──────────────
const lines = () => {
  const heads = [null, "arrow", "bar", "dot", "circle", "triangle", "diamond", "triangle_outline", "circle_outline", "diamond_outline"];
  const out = heads.map((endArrowhead, i) =>
    linear("arrow", 40, 40 + i * 40, [[0, 0], [200, 0]], { endArrowhead, startArrowhead: i % 2 ? "dot" : null }),
  );
  out.push(linear("arrow", 320, 40, [[0, 0], [120, 80], [240, 0]], {}));
  out.push(linear("arrow", 320, 160, [[0, 0], [120, 80], [240, 0]], { roundness: null }));
  out.push(linear("line", 320, 280, [[0, 0], [80, 60], [160, -20], [240, 40]]));
  out.push(linear("line", 320, 360, [[0, 0], [100, 0], [60, 80], [0, 0]], { roundness: null, backgroundColor: "#b2f2bb", fillStyle: "hachure" }));
  out.push(linear("arrow", 480, 360, [[0, 0], [120, 60]], { strokeStyle: "dashed", strokeWidth: 4, strokeColor: "#e03131" }));
  out.push(linear("arrow", 480, 460, [[0, 0], [120, 60]], { strokeStyle: "dotted", roughness: 0 }));
  return out;
};

// ── freedraw: pressure-sensitive and simulated strokes ──────────────────────
const freedraw = () => {
  const wave = (n, f) => Array.from({ length: n }, (_, i) => f(i / (n - 1)));
  const stroke = (x, y, points, props) =>
    base("freedraw", x, y, 0, 0, { points, pressures: [], simulatePressure: true, lastCommittedPoint: null, ...props });
  return [
    stroke(40, 80, wave(60, (t) => [t * 300, Math.sin(t * Math.PI * 4) * 30]), {}),
    stroke(40, 200, wave(60, (t) => [t * 300, Math.sin(t * Math.PI * 2) * 30]), {
      simulatePressure: false,
      pressures: wave(60, (t) => 0.2 + 0.8 * Math.sin(t * Math.PI)),
    }),
    stroke(40, 300, wave(40, (t) => [Math.cos(t * Math.PI * 2) * 60 + 60, Math.sin(t * Math.PI * 2) * 40]), {
      strokeWidth: 4,
      strokeColor: "#1971c2",
    }),
    stroke(220, 300, wave(40, (t) => [t * 150, t * t * 80]), { strokeWidth: 1, opacity: 50 }),
  ].map((e) => {
    const xs = e.points.map((p) => p[0]);
    const ys = e.points.map((p) => p[1]);
    return { ...e, width: Math.max(...xs) - Math.min(...xs), height: Math.max(...ys) - Math.min(...ys) };
  });
};

// ── text: sizes, alignment, multi-line, colour, bound text ──────────────────
const texts = () => {
  const out = [
    text(40, 40, "Small 16", { fontSize: 16 }),
    text(40, 80, "Medium 20", { fontSize: 20 }),
    text(40, 130, "Large 28", { fontSize: 28 }),
    text(40, 190, "Very large 36", { fontSize: 36 }),
    text(40, 270, "left aligned\nsecond line", { textAlign: "left" }),
    text(260, 270, "centered\nsecond line", { textAlign: "center" }),
    text(480, 270, "right\nsecond line", { textAlign: "right" }),
    text(40, 380, "Coloured äöü ß €", { strokeColor: "#e03131", fontSize: 24 }),
  ];
  const box = base("rectangle", 360, 40, 200, 100, { backgroundColor: "#ffec99", roundness: { type: 3 } });
  const label = text(0, 0, "Bound label", { containerId: box.id, textAlign: "center", verticalAlign: "middle" });
  // Centred in the box, as Excalidraw stores a label: excalidraw.com draws it
  // at its stored x/y until it is edited, the app always centres it.
  label.x = box.x + (box.width - label.width) / 2;
  label.y = box.y + (box.height - label.height) / 2;
  box.boundElements = [{ type: "text", id: label.id }];
  return [...out, box, label];
};

// ── images: an embedded PNG, plain and scaled/cropped-free ──────────────────
const png = (size) => {
  const row = (y) =>
    [0, ...Array.from({ length: size }, (_, x) => ((x >> 2) + (y >> 2)) % 2 ? [25, 113, 194] : [255, 212, 59]).flat()];
  const raw = Buffer.from(Array.from({ length: size }, (_, y) => row(y)).flat());
  const chunk = (type, data) => {
    const body = Buffer.concat([Buffer.from(type), data]);
    const len = Buffer.alloc(4);
    len.writeUInt32BE(data.length);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body));
    return Buffer.concat([len, body, crc]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr.set([8, 2, 0, 0, 0], 8);
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw)),
    chunk("IEND", Buffer.alloc(0)),
  ]);
};

const images = () => {
  const fileId = "checker-32";
  const dataURL = `data:image/png;base64,${png(32).toString("base64")}`;
  const image = (x, y, w, h, props = {}) =>
    base("image", x, y, w, h, { fileId, status: "saved", scale: [1, 1], crop: null, strokeColor: "transparent", ...props });
  return drawing(
    [
      image(40, 40, 160, 160),
      image(240, 40, 240, 120),
      image(520, 40, 120, 120, { opacity: 50 }),
      image(40, 240, 160, 160, { angle: Math.PI / 8 }),
      image(240, 240, 160, 160, { scale: [-1, 1] }),
      base("rectangle", 440, 240, 160, 160, { strokeColor: "#e03131" }),
    ],
    { [fileId]: { mimeType: "image/png", id: fileId, dataURL, created: 1, lastRetrieved: 1 } },
  );
};

const fixtures = {
  shapes: () => drawing(shapes()),
  strokes: () => drawing(strokes()),
  lines: () => drawing(lines()),
  freedraw: () => drawing(freedraw()),
  text: () => drawing(texts()),
  images,
};

mkdirSync(`${here}fixtures`, { recursive: true });
for (const [name, build] of Object.entries(fixtures)) {
  counter = 0;
  writeFileSync(`${here}fixtures/${name}.excalidraw`, `${JSON.stringify(build(), null, 2)}\n`);
}
console.log(`wrote ${Object.keys(fixtures).length} fixtures`);
