// Regenerates the committed Benchmark Scenes (see docs/performance-benchmarks.md).
// Deterministic: same output on every run, so diffs only show intended changes.
// Usage: node benchmarks/generate.mjs
import { writeFileSync } from "node:fs";

const SIZES = [1000, 5000];
const COLUMNS = 100;
const CELL = 120;
const TYPES = ["rectangle", "ellipse", "diamond", "line", "arrow", "freedraw", "text"];
const COLORS = ["#1e1e1e", "#e03131", "#2f9e44", "#1971c2", "#f08c00"];
const FILLS = ["hachure", "cross-hatch", "solid"];

function scene(count) {
  let state = 1;
  const rand = () => (state = (state * 48271) % 2147483647) / 2147483647;
  const elements = [];
  for (let i = 0; i < count; i++) {
    const type = TYPES[i % TYPES.length];
    const x = (i % COLUMNS) * CELL;
    const y = Math.floor(i / COLUMNS) * CELL;
    const w = 40 + Math.round(rand() * 60);
    const h = 40 + Math.round(rand() * 60);
    const e = {
      id: `bench-${i}`,
      type,
      x,
      y,
      width: w,
      height: h,
      strokeColor: COLORS[i % COLORS.length],
      backgroundColor: i % 3 === 0 ? "transparent" : "#a5d8ff",
      fillStyle: FILLS[i % FILLS.length],
      roughness: i % 4 === 0 ? 0 : 1,
      seed: 1 + Math.floor(rand() * 2 ** 31),
    };
    if (type === "line" || type === "arrow") {
      e.points = [[0, 0], [w / 2, h], [w, 0]];
    } else if (type === "freedraw") {
      e.points = Array.from({ length: 24 }, (_, k) => [
        (k / 23) * w,
        h / 2 + Math.sin(k / 3) * (h / 2),
      ]);
      e.pressures = [];
      e.simulatePressure = true;
    } else if (type === "text") {
      e.text = `Element ${i}`;
      e.originalText = e.text;
      e.fontSize = 20;
      e.fontFamily = 5;
      e.height = 25;
    }
    elements.push(e);
  }
  return { type: "excalidraw", version: 2, source: "benchmarks/generate.mjs", elements, appState: { viewBackgroundColor: "#ffffff" }, files: {} };
}

for (const count of SIZES) {
  const path = new URL(`scene-${count}.excalidraw`, import.meta.url);
  writeFileSync(path, JSON.stringify(scene(count)) + "\n");
}
